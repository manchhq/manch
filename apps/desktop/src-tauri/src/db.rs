//! SQLite-backed store for user-provided provider API keys.
//! Inline here for the first slice; extract into `manch-memory` later.

use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;

/// Owns the SQLite connection behind a mutex so it can live in Tauri state.
pub struct Db(Mutex<Connection>);

impl Db {
    /// Open (or create) the database file and ensure the schema exists.
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(&conn)?;
        Ok(Db(Mutex::new(conn)))
    }

    /// In-memory database, for tests.
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(&conn)?;
        Ok(Db(Mutex::new(conn)))
    }

    fn init(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS provider_keys (
                 provider TEXT PRIMARY KEY,
                 api_key  TEXT NOT NULL
             )",
            [],
        )?;
        Ok(())
    }

    /// Insert or replace the key for a provider.
    pub fn save_key(&self, provider: &str, api_key: &str) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO provider_keys (provider, api_key) VALUES (?1, ?2)
             ON CONFLICT(provider) DO UPDATE SET api_key = excluded.api_key",
            rusqlite::params![provider, api_key],
        )?;
        Ok(())
    }

    /// Fetch the stored key for a provider, if any.
    pub fn get_key(&self, provider: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT api_key FROM provider_keys WHERE provider = ?1",
            [provider],
            |row| row.get(0),
        )
        .optional()
    }

    /// All providers that have a saved key, sorted.
    pub fn list_providers(&self) -> rusqlite::Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT provider FROM provider_keys ORDER BY provider")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_get_roundtrips() {
        let db = Db::open_in_memory().unwrap();
        db.save_key("anthropic", "sk-ant-123").unwrap();
        assert_eq!(db.get_key("anthropic").unwrap().as_deref(), Some("sk-ant-123"));
    }

    #[test]
    fn get_missing_returns_none() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_key("gemini").unwrap(), None);
    }

    #[test]
    fn save_twice_overwrites() {
        let db = Db::open_in_memory().unwrap();
        db.save_key("anthropic", "old").unwrap();
        db.save_key("anthropic", "new").unwrap();
        assert_eq!(db.get_key("anthropic").unwrap().as_deref(), Some("new"));
    }

    #[test]
    fn list_providers_returns_saved_sorted() {
        let db = Db::open_in_memory().unwrap();
        db.save_key("gemini", "g").unwrap();
        db.save_key("anthropic", "a").unwrap();
        assert_eq!(db.list_providers().unwrap(), vec!["anthropic", "gemini"]);
    }
}
