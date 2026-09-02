use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::acp::ContentBlock;
use crate::{Context, Result, Role};

/// One role-attributed span of the conversation: contiguous same-role blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub role: Role,
    pub blocks: Vec<ContentBlock>,
}

/// Fold an ordered `(role, block)` log into [`Turn`]s by merging runs of the
/// same role. The one place turn-grouping lives, so every [`MemoryStore`]
/// coalesces identically.
pub fn coalesce_turns(items: impl IntoIterator<Item = (Role, ContentBlock)>) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for (role, block) in items {
        match turns.last_mut() {
            Some(last) if last.role == role => last.blocks.push(block),
            _ => turns.push(Turn {
                role,
                blocks: vec![block],
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
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Append a role-tagged content block to a session's append-only history.
    async fn append(&self, session_id: &str, role: Role, block: ContentBlock) -> Result<()>;

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
        let b = |s: &str| ContentBlock::Text(TextContent::new(s.to_string()));
        let turns = coalesce_turns([
            (Role::User, b("a")),
            (Role::User, b("b")),
            (Role::Assistant, b("c")),
            (Role::User, b("d")),
        ]);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, Role::User);
        assert_eq!(turns[0].blocks.len(), 2);
        assert_eq!(turns[1].role, Role::Assistant);
        assert_eq!(turns[2].role, Role::User);
        assert_eq!(turns[2].blocks.len(), 1);
    }
}
