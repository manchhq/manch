//! SQLite-backed store for user-provided provider API keys.
//! Inline here for the first slice; extract into `manch-memory` later.

use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn new_id(prefix: &str) -> String {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}{t:x}{n:x}")
}

fn row_to_team(r: &rusqlite::Row<'_>) -> rusqlite::Result<manch_dto::Team> {
    let members_json: String = r.get(4)?;
    let caps_json: String = r.get(5)?;
    let members: Vec<manch_dto::TeamMember> =
        serde_json::from_str(&members_json).unwrap_or_default();
    let capabilities: Vec<String> = serde_json::from_str(&caps_json).unwrap_or_default();
    Ok(manch_dto::Team {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        problem: r.get(3)?,
        members,
        capabilities,
    })
}

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
    #[cfg(test)]
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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS workspaces (
                 id          TEXT PRIMARY KEY,
                 name        TEXT NOT NULL,
                 description TEXT NOT NULL DEFAULT ''
             )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS teams (
                 id           TEXT PRIMARY KEY,
                 workspace_id TEXT NOT NULL,
                 name         TEXT NOT NULL,
                 problem      TEXT NOT NULL DEFAULT '',
                 members      TEXT NOT NULL DEFAULT '[]',
                 capabilities TEXT NOT NULL DEFAULT '[]'
             )",
            [],
        )?;
        Ok(())
    }

    pub fn list_workspaces(&self) -> rusqlite::Result<Vec<manch_dto::Workspace>> {
        let conn = self.0.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, name, description FROM workspaces ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(manch_dto::Workspace {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn create_workspace(
        &self,
        name: &str,
        description: &str,
    ) -> rusqlite::Result<manch_dto::Workspace> {
        let id = new_id("ws_");
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO workspaces (id, name, description) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, description],
        )?;
        Ok(manch_dto::Workspace {
            id,
            name: name.into(),
            description: description.into(),
        })
    }

    pub fn rename_workspace(&self, id: &str, name: &str) -> rusqlite::Result<manch_dto::Workspace> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "UPDATE workspaces SET name = ?2 WHERE id = ?1",
            rusqlite::params![id, name],
        )?;
        conn.query_row(
            "SELECT id, name, description FROM workspaces WHERE id = ?1",
            [id],
            |r| {
                Ok(manch_dto::Workspace {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                })
            },
        )
    }

    pub fn delete_workspace(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn seed_defaults(&self) -> rusqlite::Result<()> {
        if self.list_workspaces()?.is_empty() {
            self.create_workspace("My workspace", "Default workspace")?;
        }
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

    pub fn create_team(&self, input: manch_dto::CreateTeam) -> rusqlite::Result<manch_dto::Team> {
        let id = new_id("tm_");
        let members = if input.auto && input.members.is_empty() {
            vec![
                manch_dto::TeamMember {
                    role: "Researcher".into(),
                    provider: "anthropic".into(),
                },
                manch_dto::TeamMember {
                    role: "Analyst".into(),
                    provider: "anthropic".into(),
                },
                manch_dto::TeamMember {
                    role: "Critic".into(),
                    provider: "claude-code".into(),
                },
            ]
        } else {
            input.members
        };
        let capabilities = vec![
            "read_file".to_string(),
            "search".to_string(),
            "write_report".to_string(),
        ];
        let members_json = serde_json::to_string(&members).unwrap();
        let caps_json = serde_json::to_string(&capabilities).unwrap();
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO teams (id, workspace_id, name, problem, members, capabilities) VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![id, input.workspace_id, input.name, input.problem, members_json, caps_json],
        )?;
        Ok(manch_dto::Team {
            id,
            workspace_id: input.workspace_id,
            name: input.name,
            problem: input.problem,
            members,
            capabilities,
        })
    }

    pub fn list_teams(&self, workspace_id: &str) -> rusqlite::Result<Vec<manch_dto::Team>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workspace_id, name, problem, members, capabilities FROM teams WHERE workspace_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map([workspace_id], row_to_team)?;
        rows.collect()
    }

    pub fn get_team(&self, id: &str) -> rusqlite::Result<Option<manch_dto::Team>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT id, workspace_id, name, problem, members, capabilities FROM teams WHERE id = ?1",
            [id],
            row_to_team,
        )
        .optional()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_crud_roundtrips() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.list_workspaces().unwrap().is_empty());
        let w = db.create_workspace("Legal", "case work").unwrap();
        assert_eq!(w.name, "Legal");
        let all = db.list_workspaces().unwrap();
        assert_eq!(all.len(), 1);
        let renamed = db.rename_workspace(&w.id, "Legal research").unwrap();
        assert_eq!(renamed.name, "Legal research");
        db.delete_workspace(&w.id).unwrap();
        assert!(db.list_workspaces().unwrap().is_empty());
    }

    #[test]
    fn seed_inserts_one_default_workspace_only_when_empty() {
        let db = Db::open_in_memory().unwrap();
        db.seed_defaults().unwrap();
        assert_eq!(db.list_workspaces().unwrap().len(), 1);
        db.seed_defaults().unwrap(); // idempotent
        assert_eq!(db.list_workspaces().unwrap().len(), 1);
    }

    #[test]
    fn save_then_get_roundtrips() {
        let db = Db::open_in_memory().unwrap();
        db.save_key("anthropic", "sk-ant-123").unwrap();
        assert_eq!(
            db.get_key("anthropic").unwrap().as_deref(),
            Some("sk-ant-123")
        );
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

    #[test]
    fn team_crud_with_members_roundtrips() {
        let db = Db::open_in_memory().unwrap();
        let ws = db.create_workspace("w", "").unwrap();
        let input = manch_dto::CreateTeam {
            workspace_id: ws.id.clone(),
            name: "Discovery".into(),
            problem: "find precedent".into(),
            auto: false,
            members: vec![manch_dto::TeamMember {
                role: "researcher".into(),
                provider: "anthropic".into(),
            }],
        };
        let team = db.create_team(input).unwrap();
        assert_eq!(team.members.len(), 1);
        let got = db.get_team(&team.id).unwrap().unwrap();
        assert_eq!(got.name, "Discovery");
        assert_eq!(db.list_teams(&ws.id).unwrap().len(), 1);
    }
}
