//! # manch-core
//!
//! The Manch runtime: Agent/Tool/Channel registries + the prompt/tool loop.
//! Framework-free, domain-free — the seam gate. Implements
//! [`manch_protocol::PromptHandler`] over registered [`manch_protocol::Agent`]s,
//! [`manch_protocol::Tool`]s, and a [`manch_protocol::MemoryStore`].

#[cfg(test)]
mod testing;
