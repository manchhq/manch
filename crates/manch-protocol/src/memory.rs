use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::acp::{ContentBlock, ToolCallContent};
use crate::{Context, Result, Role, ToolInvocation};

/// One item in a turn's history. Widened from a bare `ContentBlock` because
/// ACP's content vocabulary (`Text | Image | Audio | Resource`) has nowhere to
/// put a tool call: without this, the assistant's `tool_use` request is never
/// persisted, and a provider that requires a `tool_use` to precede its
/// `tool_result` (Anthropic) would reject a second loop iteration built from
/// stored history.
// `ContentBlock` is ACP's own, much larger type; `ToolInvocation` and the
// `ToolResult` fields are small by comparison. Boxing `Block`'s payload to
// close that gap would ripple `Box`/deref through every call site across the
// workspace that matches on `Entry` — out of proportion to the lint. Sizes
// are known at each match, so the variance costs nothing but the enum's own
// stack slot (same rationale as `AgentEvent`, see `lib.rs`).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// Ordinary ACP content — the only kind of entry that existed before this
    /// type widened past `ContentBlock`.
    Block(ContentBlock),
    /// The model's request to run a host-registered tool (BYOK path).
    ToolCall(ToolInvocation),
    /// A tool's result, addressed back to the call that produced it.
    ToolResult {
        id: String,
        content: Vec<ToolCallContent>,
        /// The tool's structured result, carried for whoever is watching the
        /// turn. Never sent to a provider — see the note on [`MemoryStore`].
        ///
        /// A durable store will hold histories written before this field
        /// existed, and they must still load. That is tested, and it holds
        /// because serde fills a missing `Option` with `None` on its own —
        /// see [`ToolInvocation::provider_meta`](crate::ToolInvocation) for
        /// which of the two attributes below is actually doing the work.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw_output: Option<serde_json::Value>,
    },
}

/// One role-attributed span of the conversation: contiguous same-role entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub role: Role,
    pub entries: Vec<Entry>,
}

/// Fold an ordered `(role, entry)` log into [`Turn`]s by merging runs of the
/// same role. The one place turn-grouping lives, so every [`MemoryStore`]
/// coalesces identically.
pub fn coalesce_turns(items: impl IntoIterator<Item = (Role, Entry)>) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for (role, entry) in items {
        match turns.last_mut() {
            Some(last) if last.role == role => last.entries.push(entry),
            _ => turns.push(Turn {
                role,
                entries: vec![entry],
            }),
        }
    }
    turns
}

/// **Extension point 4.** How sessions persist and how context is assembled. ACP
/// deliberately does not cover persistence, so this is wholly Manch's.
///
/// Implementations: SQLite default (`manch-memory`); swap for Postgres or a
/// retrieval-backed strategy.
///
/// # `Entry::ToolResult::raw_output` and durable stores
///
/// A tool's structured result reaches the transcript, so a consumer whose tools
/// return regulated data now holds that data in two places rather than one. An
/// implementation is free to drop `raw_output` on `append` — and this order is
/// deliberate: carrying it by default and letting a store decline is
/// recoverable, whereas never carrying it cannot be undone by the store.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Append a role-tagged entry to a session's append-only history.
    async fn append(&self, session_id: &str, role: Role, entry: Entry) -> Result<()>;

    /// Assemble the context for the next turn. **The seam** — retrieval,
    /// summarisation, and compaction all live behind this one method.
    async fn assemble_context(&self, session_id: &str) -> Result<Context>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_merges_runs_of_same_role() {
        use crate::acp::TextContent;
        let b = |s: &str| Entry::Block(ContentBlock::Text(TextContent::new(s.to_string())));
        let turns = coalesce_turns([
            (Role::User, b("a")),
            (Role::User, b("b")),
            (Role::Assistant, b("c")),
            (Role::User, b("d")),
        ]);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, Role::User);
        assert_eq!(turns[0].entries.len(), 2);
        assert_eq!(turns[1].role, Role::Assistant);
        assert_eq!(turns[2].role, Role::User);
        assert_eq!(turns[2].entries.len(), 1);
    }

    #[test]
    fn coalesce_merges_runs_of_the_same_role_across_entry_kinds() {
        use crate::ToolInvocation;
        use crate::acp::TextContent;
        let b = |s: &str| Entry::Block(ContentBlock::Text(TextContent::new(s.to_string())));
        let call = Entry::ToolCall(ToolInvocation {
            id: "c1".into(),
            name: "t".into(),
            arguments: serde_json::Value::Null,
            provider_meta: None,
        });
        let turns = coalesce_turns([
            (Role::User, b("hi")),
            (Role::Assistant, b("thinking")),
            (Role::Assistant, call),
            (
                Role::User,
                Entry::ToolResult {
                    id: "c1".into(),
                    content: vec![],
                    raw_output: None,
                },
            ),
        ]);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[1].role, Role::Assistant);
        assert_eq!(
            turns[1].entries.len(),
            2,
            "text and tool call are one assistant turn"
        );
    }

    #[test]
    fn a_system_run_never_absorbs_the_user_turn_beside_it() {
        // The fold merges runs of the *same* role, so this already holds — but
        // it is the property that makes a system prompt worth having, so it is
        // pinned rather than assumed. If system content merged into the user's
        // turn it would arrive carrying the user's authority, which is the bug
        // `Role::System` exists to prevent.
        use crate::acp::TextContent;
        let b = |s: &str| Entry::Block(ContentBlock::Text(TextContent::new(s.to_string())));
        let turns = coalesce_turns(vec![
            (Role::System, b("you are a careful lawyer")),
            (Role::System, b("never invent citations")),
            (Role::User, b("summarise this")),
        ]);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, Role::System);
        assert_eq!(
            turns[0].entries.len(),
            2,
            "adjacent system entries coalesce"
        );
        assert_eq!(turns[1].role, Role::User);
    }

    #[test]
    fn a_tool_result_stored_before_raw_output_existed_still_deserialises() {
        // Same guarantee as `ToolInvocation::provider_meta`: a durable
        // MemoryStore holds histories written before this field. Without
        // serde(default), adopting it breaks every stored conversation.
        let stored = r#"{"ToolResult":{"id":"c1","content":[]}}"#;
        let entry: Entry = serde_json::from_str(stored).expect("old histories must load");
        match entry {
            Entry::ToolResult { id, raw_output, .. } => {
                assert_eq!(id, "c1");
                assert!(raw_output.is_none());
            }
            other => panic!("expected a ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_result_without_a_structured_result_does_not_serialise_the_key() {
        // An absent key and an explicit null are different facts to anything
        // reading the transcript back.
        let entry = Entry::ToolResult {
            id: "c1".to_string(),
            content: vec![],
            raw_output: None,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert!(json["ToolResult"].get("raw_output").is_none());
    }
}
