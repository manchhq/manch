//! HTTP policy against a real socket: retry, backoff and stall detection.
//!
//! These are not live-provider tests and are **not** `#[ignore]`d — they run a
//! throwaway listener on loopback. That matters, because the behaviour under
//! test only exists between "the request was sent" and "the response arrived",
//! which no pure parser test can reach and no captured body can reproduce.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use manch_llm::AnthropicAgent;
use manch_protocol::acp::{ContentBlock, StopReason, TextContent};
use manch_protocol::{Agent, AgentEvent, Context, Entry, EventSink, Result, Role, Turn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct Blackhole;

#[async_trait]
impl EventSink for Blackhole {
    async fn emit(&self, _event: AgentEvent) -> Result<()> {
        Ok(())
    }
}

fn ask() -> Context {
    Context {
        session_id: "http-policy".to_string(),
        turns: vec![Turn {
            role: Role::User,
            entries: vec![Entry::Block(ContentBlock::Text(TextContent::new(
                "hi".to_string(),
            )))],
        }],
    }
}

const OK_SSE: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// Serve `script` in order, one response per connection, counting requests.
/// The last entry repeats once the script runs out. `None` means "accept the
/// request and never answer" — the stall this whole module exists for.
async fn server(script: Vec<Option<&'static str>>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let n = seen.fetch_add(1, Ordering::SeqCst);
            let reply = *script.get(n).unwrap_or_else(|| script.last().unwrap());
            tokio::spawn(async move {
                // Drain what the client sends; we never need to parse it.
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                match reply {
                    Some(bytes) => {
                        let _ = sock.write_all(bytes.as_bytes()).await;
                        let _ = sock.flush().await;
                    }
                    // Hold the connection open, saying nothing at all.
                    None => {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                }
            });
        }
    });

    (format!("http://{addr}/v1"), hits)
}

#[tokio::test]
async fn a_rate_limited_request_is_retried_and_then_succeeds() {
    let (base, hits) = server(vec![
        Some("HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        Some(OK_SSE),
    ])
    .await;

    let stop = AnthropicAgent::new("k".into(), None)
        .base_url(base)
        .prompt(ask(), &[], Arc::new(Blackhole))
        .await
        .expect("a 429 followed by a 200 must succeed, not fail the turn");

    assert_eq!(stop, StopReason::EndTurn);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "the 429 must be retried once"
    );
}

#[tokio::test]
async fn retries_are_bounded_and_the_error_still_reaches_the_host() {
    let (base, hits) = server(vec![Some(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )])
    .await;

    let err = AnthropicAgent::new("k".into(), None)
        .base_url(base)
        .max_retries(2)
        .prompt(ask(), &[], Arc::new(Blackhole))
        .await
        .unwrap_err();

    assert!(err.to_string().contains("503"), "got {err}");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        3,
        "one attempt plus two retries, then stop — not an unbounded loop"
    );
}

#[tokio::test]
async fn a_client_error_is_not_retried() {
    // A bad key or a malformed body fails identically however many times it is
    // sent; retrying only delays the error the host needs to see.
    let (base, hits) = server(vec![Some(
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )])
    .await;

    let _ = AnthropicAgent::new("k".into(), None)
        .base_url(base)
        .prompt(ask(), &[], Arc::new(Blackhole))
        .await
        .unwrap_err();

    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "401 must be sent exactly once"
    );
}

#[tokio::test]
async fn a_stalled_provider_times_out_instead_of_hanging_forever() {
    // The failure this issue is really about: the connection is accepted and
    // then nothing happens. Before a read timeout existed this hung the caller
    // indefinitely — a frozen desktop UI with no error to show.
    let (base, _) = server(vec![None]).await;

    let started = Instant::now();
    let err = AnthropicAgent::new("k".into(), None)
        .base_url(base)
        .read_timeout(Duration::from_millis(300))
        .max_retries(0)
        .prompt(ask(), &[], Arc::new(Blackhole))
        .await
        .unwrap_err();

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the call must give up quickly, not hang"
    );
    assert!(
        matches!(err, manch_protocol::Error::Timeout(_)),
        "a stall must be distinguishable from a provider error, got {err:?}"
    );
}

#[tokio::test]
async fn a_retry_after_header_is_honoured() {
    let (base, hits) = server(vec![
        Some("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        Some(OK_SSE),
    ])
    .await;

    let started = Instant::now();
    AnthropicAgent::new("k".into(), None)
        .base_url(base)
        .prompt(ask(), &[], Arc::new(Blackhole))
        .await
        .expect("the retry must succeed");

    assert_eq!(hits.load(Ordering::SeqCst), 2);
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "the provider asked for a second; backoff alone would have waited 500ms"
    );
}
