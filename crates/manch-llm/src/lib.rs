//! BYOK provider clients for Manch — direct provider HTTP/SSE, no execution surface.
//! Each provider implements `manch_protocol::Agent` and emits ACP event vocabulary.

use manch_protocol::Context;
use manch_protocol::acp::ContentBlock;

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "anthropic")]
pub use anthropic::AnthropicAgent;

#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "gemini")]
pub use gemini::GeminiAgent;

/// A model advertised by a provider's list-models endpoint.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
}

/// One parsed SSE line: streamed text, or a surfaced error message.
pub(crate) enum SseItem {
    Text(String),
    Error(String),
}

/// Drain complete `\n`-terminated lines from `buf`, applying `parse` to each
/// line's trimmed `data:` payload. Any trailing partial line is retained in
/// `buf`. Splitting on the ASCII `\n` byte keeps multibyte UTF-8 sequences
/// (Devanagari, CJK, emoji) whole across network chunk boundaries.
pub(crate) fn drain_sse(
    buf: &mut Vec<u8>,
    parse: impl Fn(&str) -> Option<SseItem>,
) -> Vec<SseItem> {
    let mut out = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line_bytes);
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:")
            && let Some(item) = parse(data.trim())
        {
            out.push(item);
        }
    }
    out
}

/// Flatten a `Context` into a single prompt string (concatenated user text
/// blocks). Placeholder until real memory/multi-turn assembly lands.
pub(crate) fn prompt_text(ctx: &Context) -> String {
    ctx.blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Install the `ring` rustls crypto provider once (reqwest `rustls-no-provider`
/// ships no backend; first `Client` build would panic otherwise).
pub(crate) fn ensure_crypto_provider() {
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Map any error into `manch_protocol::Error::Other`.
pub(crate) fn err(e: impl ToString) -> manch_protocol::Error {
    manch_protocol::Error::Other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use manch_protocol::Context;
    use manch_protocol::acp::{ContentBlock, TextContent};

    #[test]
    fn drain_sse_extracts_data_lines_and_leaves_partial() {
        let mut buf = b"data: {\"t\":1}\ndata: partial".to_vec();
        let items = drain_sse(&mut buf, |d| Some(SseItem::Text(d.to_string())));
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], SseItem::Text(s) if s == "{\"t\":1}"));
        assert_eq!(String::from_utf8_lossy(&buf), "data: partial"); // partial retained
    }

    #[test]
    fn prompt_text_joins_user_text_blocks() {
        let ctx = Context {
            session_id: "s1".into(),
            blocks: vec![
                ContentBlock::Text(TextContent::new("hello".to_string())),
                ContentBlock::Text(TextContent::new("world".to_string())),
            ],
        };
        assert_eq!(prompt_text(&ctx), "hello\nworld");
    }
}
