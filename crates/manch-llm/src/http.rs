//! Shared HTTP behaviour for every provider: one pooled client, deadlines that
//! suit a *streamed* response, and bounded retry on the statuses worth retrying.
//!
//! All of it lived nowhere before — each call built `reqwest::Client::new()`,
//! which has **no default timeout**, so a provider that accepted the connection
//! and then stalled hung the caller forever with no way for a host to impose a
//! deadline through `Agent::prompt`.

use std::sync::OnceLock;
use std::time::Duration;

use crate::err;

/// Time allowed to establish a connection. Short: failing to connect is a fast,
/// unambiguous failure, and waiting longer never turns it into a success.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum time between two reads on a live response — **not** a deadline for
/// the whole turn.
///
/// The distinction is the whole point. `reqwest`'s `timeout()` is a total
/// deadline covering the response body, so on a streamed turn it would cut off
/// a perfectly healthy long generation partway through. `read_timeout()` resets
/// after every successful read, so it bounds *stalls* while leaving a slow
/// answer alone.
///
/// Two minutes rather than seconds because the first read is the slowest: a
/// thinking model does all of its reasoning before the first token appears, and
/// a long document makes that longer still.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(120);

/// Retries *after* the first attempt, so the default is three tries in total.
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// Base for exponential backoff: 500ms, then 1s, then 2s…
const BACKOFF_BASE: Duration = Duration::from_millis(500);

/// A provider's HTTP client plus the policy applied to every request through it.
///
/// Held by an agent and cloned cheaply — `reqwest::Client` *is* the connection
/// pool, so building one per call (as every provider used to) threw away
/// pooling and paid for a fresh TLS handshake on every request. On a per-page
/// OCR fan-out that is a real cost, repeated, for nothing.
#[derive(Debug, Clone)]
pub(crate) struct Http {
    client: reqwest::Client,
    connect_timeout: Duration,
    read_timeout: Duration,
    max_retries: u32,
}

impl Default for Http {
    fn default() -> Self {
        Self::build(
            DEFAULT_CONNECT_TIMEOUT,
            DEFAULT_READ_TIMEOUT,
            DEFAULT_MAX_RETRIES,
        )
    }
}

impl Http {
    fn build(connect_timeout: Duration, read_timeout: Duration, max_retries: u32) -> Self {
        crate::ensure_crypto_provider();
        let client = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .read_timeout(read_timeout)
            .build()
            // The builder only fails on TLS/resolver setup, which
            // `Client::new()` would hit identically — so falling back keeps a
            // construction path that cannot panic, rather than pretending the
            // error is actionable here.
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            connect_timeout,
            read_timeout,
            max_retries,
        }
    }

    /// Rebuilds the client: the timeouts are baked into it at construction.
    #[must_use]
    pub(crate) fn with_connect_timeout(self, d: Duration) -> Self {
        Self::build(d, self.read_timeout, self.max_retries)
    }

    #[must_use]
    pub(crate) fn with_read_timeout(self, d: Duration) -> Self {
        Self::build(self.connect_timeout, d, self.max_retries)
    }

    /// Retries after the first attempt. `0` disables retrying.
    #[must_use]
    pub(crate) fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Send a request, retrying the statuses worth retrying.
    ///
    /// **Retry happens only before any bytes have been streamed.** `send`
    /// resolves once the response *headers* arrive, so every retry here
    /// replaces a request that produced no output. A stream that breaks partway
    /// through is deliberately not retried: the sink has already been given
    /// text, and a second attempt would emit it twice.
    pub(crate) async fn send(
        &self,
        req: reqwest::RequestBuilder,
    ) -> manch_protocol::Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            // A body that cannot be cloned cannot be replayed, so such a
            // request gets exactly one attempt rather than a silent half-retry.
            let Some(this_try) = req.try_clone() else {
                return req.send().await.map_err(transport_err);
            };
            let remaining = self.max_retries.saturating_sub(attempt);
            match this_try.send().await {
                Ok(resp) if should_retry(resp.status()) && remaining > 0 => {
                    let wait = retry_after(resp.headers()).unwrap_or_else(|| backoff(attempt));
                    tokio::time::sleep(wait).await;
                }
                // A connect or read timeout on the way to the headers is worth
                // one more try; anything else is the provider's answer.
                Err(e) if remaining > 0 && (e.is_timeout() || e.is_connect()) => {
                    tokio::time::sleep(backoff(attempt)).await;
                }
                Ok(resp) => return Ok(resp),
                Err(e) => return Err(transport_err(e)),
            }
            attempt += 1;
        }
    }
}

/// The default client used by the free `list_models_at` functions, which have no
/// agent to read a policy from. Process-wide so the catalogue calls pool too.
pub(crate) fn shared() -> &'static Http {
    static SHARED: OnceLock<Http> = OnceLock::new();
    SHARED.get_or_init(Http::default)
}

/// Statuses worth a second attempt: rate limiting, and the 5xx family that
/// means "try later" rather than "you asked wrongly".
///
/// 4xx other than 429 is deliberately excluded — a bad key or a malformed body
/// fails identically however many times it is sent, and retrying only delays
/// the error the host needs to see.
pub(crate) fn should_retry(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Honour `Retry-After` when the provider sends one, in its delay-seconds form.
///
/// The HTTP-date form is not parsed: it needs a clock-skew-tolerant date parser
/// and no major provider sends it here. An unparsable value falls back to
/// backoff rather than being treated as zero, which would turn a rate-limit
/// response into a tight loop.
pub(crate) fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let secs: u64 = raw.trim().parse().ok()?;
    // Cap it: a provider asking for an hour should surface as an error the host
    // can see, not as a request that silently never returns.
    Some(Duration::from_secs(secs.min(60)))
}

/// Exponential backoff for `attempt` (0-based).
pub(crate) fn backoff(attempt: u32) -> Duration {
    BACKOFF_BASE * 2u32.saturating_pow(attempt.min(6))
}

/// Map a transport failure, keeping a timeout distinguishable from everything
/// else — "the network stalled" and "the model refused" want different
/// handling, and a host cannot tell them apart from a string.
pub(crate) fn transport_err(e: reqwest::Error) -> manch_protocol::Error {
    if e.is_timeout() {
        manch_protocol::Error::Timeout(e.to_string())
    } else {
        err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[test]
    fn rate_limits_and_server_errors_are_worth_retrying() {
        assert!(should_retry(StatusCode::TOO_MANY_REQUESTS));
        assert!(should_retry(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(should_retry(StatusCode::SERVICE_UNAVAILABLE));
        assert!(should_retry(StatusCode::BAD_GATEWAY));
    }

    #[test]
    fn client_errors_are_not_retried() {
        // A bad key fails identically however many times it is sent. Retrying
        // only delays the error the host needs, and on 401 it also multiplies
        // failed-auth attempts against the provider.
        assert!(!should_retry(StatusCode::UNAUTHORIZED));
        assert!(!should_retry(StatusCode::BAD_REQUEST));
        assert!(!should_retry(StatusCode::NOT_FOUND));
        assert!(!should_retry(StatusCode::OK));
    }

    fn headers(v: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_str(v).unwrap());
        h
    }

    #[test]
    fn retry_after_seconds_are_honoured() {
        assert_eq!(retry_after(&headers("3")), Some(Duration::from_secs(3)));
    }

    #[test]
    fn an_absent_or_unparsable_retry_after_falls_back_to_backoff() {
        // Notably *not* zero: reading a date form as 0 would turn a rate-limit
        // response into a tight loop against a provider already saying stop.
        assert_eq!(retry_after(&HeaderMap::new()), None);
        assert_eq!(retry_after(&headers("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(retry_after(&headers("soon")), None);
    }

    #[test]
    fn a_very_long_retry_after_is_capped() {
        // A provider asking for an hour should surface as an error the host can
        // see, not as a call that silently never returns.
        assert_eq!(retry_after(&headers("3600")), Some(Duration::from_secs(60)));
    }

    #[test]
    fn backoff_grows_and_never_overflows() {
        assert!(backoff(0) < backoff(1));
        assert!(backoff(1) < backoff(2));
        // `2^attempt` on a large attempt count must not panic in debug.
        let _ = backoff(u32::MAX);
    }
}
