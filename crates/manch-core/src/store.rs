//! The reference [`MemoryStore`] implementation: an in-process, in-memory log
//! of role-tagged [`Entry`] values, coalesced into [`Turn`]s on read.
//!
//! `MemStore` is the store [`crate::Manch::builder`] reaches for when nothing
//! more durable is wired up — it makes `Entry` handling and turn coalescing
//! concrete and correct, and it's what this crate's own tests and examples
//! build on. It is **not** a production persistence layer: everything lives
//! in a `Mutex<Vec<_>>`, so a process restart (or crash) loses every session
//! it holds. A durable store — SQLite, Postgres, or anything that survives a
//! restart — is a separate concern, left to another crate that implements the
//! same [`MemoryStore`] trait.

use std::sync::Mutex;

use async_trait::async_trait;
use manch_protocol::{Context, Entry, MemoryStore, Result, Role, coalesce_turns};

/// An in-memory, single-process [`MemoryStore`]. See the module docs for what
/// this is (and is not) suitable for.
#[derive(Default)]
pub struct MemStore {
    entries: Mutex<Vec<(Role, Entry)>>,
}

impl MemStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of raw appended entries (not turns). An inspection helper
    /// for tests and examples, not a query interface a real consumer should
    /// build on.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    /// `true` if no entries have been appended yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The raw appended entries, oldest first, so a test can assert on the
    /// ORDER a turn was persisted in and not merely how many entries it
    /// produced. An inspection helper for tests and examples, not a query
    /// interface a real consumer should build on.
    pub fn entries(&self) -> Vec<(Role, Entry)> {
        self.entries.lock().unwrap().clone()
    }
}

#[async_trait]
impl MemoryStore for MemStore {
    async fn append(&self, _session_id: &str, role: Role, entry: Entry) -> Result<()> {
        self.entries.lock().unwrap().push((role, entry));
        Ok(())
    }
    async fn assemble_context(&self, session_id: &str) -> Result<Context> {
        Ok(Context {
            session_id: session_id.to_string(),
            turns: coalesce_turns(self.entries.lock().unwrap().iter().cloned()),
        })
    }
}

#[cfg(test)]
mod tests {
    use manch_protocol::acp::{ContentBlock, TextContent};

    use super::*;

    #[tokio::test]
    async fn appended_entries_assemble_into_role_tagged_turns() {
        let store = MemStore::new();
        let b = |s: &str| Entry::Block(ContentBlock::Text(TextContent::new(s.to_string())));
        store.append("s", Role::User, b("hi")).await.unwrap();
        store
            .append("s", Role::Assistant, b("hello"))
            .await
            .unwrap();
        let ctx = store.assemble_context("s").await.unwrap();
        assert_eq!(ctx.session_id, "s");
        assert_eq!(ctx.turns.len(), 2);
        assert_eq!(ctx.turns[0].role, Role::User);
    }
}
