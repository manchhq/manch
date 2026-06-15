# Desktop BYOK Chat Window — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A working desktop chat window where the user saves a provider API key, asks "What is the capital of India?", and gets the answer.

**Architecture:** UI-first, all logic inline in `apps/desktop/src-tauri` (crates extracted later). Keys are user-provided via the UI and stored in SQLite. The BYOK call goes through `rig` (used as the completion engine). No server, no ACP, no tools, no streaming in this slice.

**Tech Stack:** Rust + Tauri v2, `rig-core` (LLM provider engine), `rusqlite` (SQLite, bundled), React 19 + TanStack Router + daisyUI/Tailwind v4, `@tauri-apps/api` (invoke).

**Spec:** `docs/superpowers/specs/2026-06-15-byok-chat-window-design.md`

---

## File Structure

**Rust (`apps/desktop/src-tauri/`):**
- `Cargo.toml` — add `rig-core`, `rusqlite`, `tokio` deps.
- `src/db.rs` — **create.** SQLite key store: `Db` wrapping `Mutex<Connection>`; `open`/`open_in_memory`/`save_key`/`get_key`/`list_providers`.
- `src/agent.rs` — **create.** `Provider` enum + rig completion call (`complete`).
- `src/commands.rs` — **create.** Tauri commands: `save_api_key`, `list_configured_providers`, `send_prompt`.
- `src/lib.rs` — **modify.** Declare modules, init `Db` in `setup`, register commands.

**Frontend (`apps/desktop/`):**
- `package.json` — add `@tauri-apps/api`.
- `src/lib/api.ts` — **create.** Typed `invoke` wrappers + `Provider` type.
- `src/components/Settings.tsx` — **create.** Key-entry form.
- `src/components/Chat.tsx` — **create.** Message list + input + provider picker.
- `src/routes/index.tsx` — **modify.** Compose Settings + Chat (replaces the version badge).

---

## Task 1: Dependencies

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/package.json`

- [ ] **Step 1: Add Rust dependencies**

Run from the repo root:

```bash
cd apps/desktop/src-tauri
cargo add rig-core
cargo add rusqlite --features bundled
cargo add tokio --features rt-multi-thread,macros
cd -
```

After it runs, the `[dependencies]` section of `apps/desktop/src-tauri/Cargo.toml` should contain entries like (exact versions will be whatever Cargo resolved — leave them):

```toml
rig-core = "..."
rusqlite = { version = "...", features = ["bundled"] }
tokio = { version = "...", features = ["rt-multi-thread", "macros"] }
```

> `rusqlite`'s `bundled` feature compiles SQLite in — no system SQLite needed. If `cargo add rig-core` resolves a version whose provider API differs from Task 3's code (e.g. `client.agent(...)` signature), adjust Task 3's code to match the resolved rig API; the shape (`ClientBuilder`/`Client::new` → `.agent(model).build()` → `agent.prompt(text).await`) is from rig's current docs.

- [ ] **Step 2: Add the frontend Tauri API package**

```bash
pnpm --filter @manch/desktop add @tauri-apps/api
```

- [ ] **Step 3: Verify the workspace still builds**

```bash
cargo build -p manch-desktop
```
Expected: builds successfully (no code changes yet, just new deps compiling).

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/package.json Cargo.lock pnpm-lock.yaml
git commit -m "build(desktop): add rig-core, rusqlite, tokio, @tauri-apps/api"
```

---

## Task 2: SQLite key store (`db.rs`)

**Files:**
- Create: `apps/desktop/src-tauri/src/db.rs`

- [ ] **Step 1: Write the failing tests**

Create `apps/desktop/src-tauri/src/db.rs` with the test module first (the impl block comes in Step 3):

```rust
//! SQLite-backed store for user-provided provider API keys.
//! Inline here for the first slice; extract into `manch-memory` later.

use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;

/// Owns the SQLite connection behind a mutex so it can live in Tauri state.
pub struct Db(Mutex<Connection>);

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
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p manch-desktop db::tests
```
Expected: FAIL to compile — `Db::open_in_memory`, `save_key`, etc. don't exist yet.

- [ ] **Step 3: Implement the store**

Insert this `impl` block in `db.rs` immediately after the `pub struct Db(...)` line (before the `#[cfg(test)]` module):

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p manch-desktop db::tests
```
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/db.rs
git commit -m "feat(desktop): SQLite store for provider API keys"
```

---

## Task 3: rig completion wrapper (`agent.rs`)

**Files:**
- Create: `apps/desktop/src-tauri/src/agent.rs`

- [ ] **Step 1: Write the failing tests** (provider mapping is pure and unit-testable; the network call is not)

Create `apps/desktop/src-tauri/src/agent.rs`:

```rust
//! BYOK completion via `rig`. The provider client sits *below* the (future)
//! manch-core loop; for this slice it just does one completion call.

use rig::completion::Prompt;
use rig::providers::{anthropic, gemini};

/// Anthropic model id (authoritative per the claude-api skill).
const ANTHROPIC_MODEL: &str = "claude-opus-4-8";
/// Gemini model id. Verify against current Gemini model names at run time.
const GEMINI_MODEL: &str = "gemini-2.5-flash";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Gemini,
}

impl Provider {
    pub fn from_id(id: &str) -> Option<Provider> {
        match id {
            "anthropic" => Some(Provider::Anthropic),
            "gemini" => Some(Provider::Gemini),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_providers_parse() {
        assert_eq!(Provider::from_id("anthropic"), Some(Provider::Anthropic));
        assert_eq!(Provider::from_id("gemini"), Some(Provider::Gemini));
    }

    #[test]
    fn unknown_provider_is_none() {
        assert_eq!(Provider::from_id("openai"), None);
        assert_eq!(Provider::from_id(""), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p manch-desktop agent::tests
```
Expected: FAIL to compile — `agent` module not declared yet in `lib.rs` (declared in Task 4) and/or `complete` missing. If the compile error is only the unused-import warning for `Prompt`/clients, that's expected until Step 3 adds `complete`.

- [ ] **Step 3: Add the completion function**

Append to `agent.rs`, after the `impl Provider` block:

```rust
/// Run one BYOK completion and return the assistant's text.
/// Errors are stringified for transport across the Tauri boundary.
pub async fn complete(provider: Provider, api_key: &str, prompt: &str) -> Result<String, String> {
    match provider {
        Provider::Anthropic => {
            let client = anthropic::ClientBuilder::new(api_key)
                .anthropic_version("2023-06-01")
                .build();
            let agent = client.agent(ANTHROPIC_MODEL).build();
            agent.prompt(prompt).await.map_err(|e| e.to_string())
        }
        Provider::Gemini => {
            let client = gemini::Client::new(api_key);
            let agent = client.agent(GEMINI_MODEL).build();
            agent.prompt(prompt).await.map_err(|e| e.to_string())
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass** (after Task 4 declares the module, the full crate test runs; for now confirm the file compiles in isolation by running the whole suite at the end of Task 4). For now, just confirm there are no syntax errors:

```bash
cargo build -p manch-desktop 2>&1 | grep -i "error\[" || echo "no hard errors yet (module wired in Task 4)"
```

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/agent.rs
git commit -m "feat(desktop): rig-backed BYOK completion (Anthropic + Gemini)"
```

---

## Task 4: Tauri commands + state wiring

**Files:**
- Create: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Create the commands**

Create `apps/desktop/src-tauri/src/commands.rs`:

```rust
//! Tauri commands the frontend invokes. Thin glue over `db` + `agent`.

use crate::agent::{complete, Provider};
use crate::db::Db;
use tauri::State;

#[tauri::command]
pub fn save_api_key(state: State<Db>, provider: String, api_key: String) -> Result<(), String> {
    if Provider::from_id(&provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }
    state.save_key(&provider, &api_key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_configured_providers(state: State<Db>) -> Result<Vec<String>, String> {
    state.list_providers().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_prompt(
    state: State<'_, Db>,
    provider: String,
    text: String,
) -> Result<String, String> {
    let prov =
        Provider::from_id(&provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    // Read the key and release the mutex guard BEFORE awaiting the network call.
    let key = state
        .get_key(&provider)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no API key saved for {provider}"))?;
    complete(prov, &key, &text).await
}
```

- [ ] **Step 2: Rewrite `lib.rs` to declare modules, init the DB, and register commands**

Replace the entire contents of `apps/desktop/src-tauri/src/lib.rs` with:

```rust
mod agent;
mod commands;
mod db;

use db::Db;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .expect("resolve app data dir");
            std::fs::create_dir_all(&dir).ok();
            let db_path = dir.join("manch.sqlite3");
            let db = Db::open(db_path.to_str().expect("utf-8 db path")).expect("open database");
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_api_key,
            commands::list_configured_providers,
            commands::send_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: Build and run the full Rust test suite**

```bash
cargo build -p manch-desktop
cargo test -p manch-desktop
```
Expected: builds; all `db::tests` and `agent::tests` pass.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): wire DB + commands (save_api_key, list_configured_providers, send_prompt)"
```

---

## Task 5: Frontend API wrapper (`api.ts`)

**Files:**
- Create: `apps/desktop/src/lib/api.ts`

- [ ] **Step 1: Create the typed invoke wrappers**

Create `apps/desktop/src/lib/api.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

export type Provider = "anthropic" | "gemini";

export const PROVIDERS: { id: Provider; label: string }[] = [
  { id: "anthropic", label: "Anthropic (Claude)" },
  { id: "gemini", label: "Google (Gemini)" },
];

/** Tauri maps camelCase JS args to snake_case Rust params (apiKey -> api_key). */
export const saveApiKey = (provider: Provider, apiKey: string): Promise<void> =>
  invoke("save_api_key", { provider, apiKey });

export const listConfiguredProviders = (): Promise<Provider[]> =>
  invoke("list_configured_providers");

export const sendPrompt = (provider: Provider, text: string): Promise<string> =>
  invoke("send_prompt", { provider, text });
```

- [ ] **Step 2: Type-check**

```bash
pnpm --filter @manch/desktop lint
```
Expected: passes (`lint` runs `tsc --noEmit`).

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/lib/api.ts
git commit -m "feat(desktop): typed Tauri command wrappers"
```

---

## Task 6: Settings component (`Settings.tsx`)

**Files:**
- Create: `apps/desktop/src/components/Settings.tsx`

- [ ] **Step 1: Create the key-entry form**

Create `apps/desktop/src/components/Settings.tsx`:

```tsx
import { useState } from "react";
import { PROVIDERS, type Provider, saveApiKey } from "../lib/api";

export function Settings({ onSaved }: { onSaved: () => void }) {
  const [provider, setProvider] = useState<Provider>("anthropic");
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await saveApiKey(provider, apiKey.trim());
      setApiKey("");
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="card bg-base-100 shadow">
      <div className="card-body gap-3">
        <h2 className="card-title">Add a provider key</h2>
        <select
          className="select select-bordered"
          value={provider}
          onChange={(e) => setProvider(e.target.value as Provider)}
        >
          {PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        <input
          type="password"
          className="input input-bordered"
          placeholder="API key"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
        />
        <button
          className="btn btn-primary"
          disabled={saving || apiKey.trim() === ""}
          onClick={save}
        >
          {saving ? "Saving…" : "Save key"}
        </button>
        {error && (
          <div role="alert" className="alert alert-error">
            <span>{error}</span>
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
pnpm --filter @manch/desktop lint
```
Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/Settings.tsx
git commit -m "feat(desktop): provider key settings form"
```

---

## Task 7: Chat component + home route

**Files:**
- Create: `apps/desktop/src/components/Chat.tsx`
- Modify: `apps/desktop/src/routes/index.tsx`

- [ ] **Step 1: Create the chat window**

Create `apps/desktop/src/components/Chat.tsx`:

```tsx
import { useState } from "react";
import { PROVIDERS, type Provider, sendPrompt } from "../lib/api";

type Msg = { role: "user" | "assistant" | "error"; text: string };

export function Chat({ providers }: { providers: Provider[] }) {
  const [provider, setProvider] = useState<Provider>(providers[0]);
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Msg[]>([]);
  const [busy, setBusy] = useState(false);

  async function send() {
    const text = input.trim();
    if (text === "" || busy) return;
    setInput("");
    setMessages((m) => [...m, { role: "user", text }]);
    setBusy(true);
    try {
      const reply = await sendPrompt(provider, text);
      setMessages((m) => [...m, { role: "assistant", text: reply }]);
    } catch (e) {
      setMessages((m) => [...m, { role: "error", text: String(e) }]);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-3 h-[70vh]">
      <select
        className="select select-bordered w-fit"
        value={provider}
        onChange={(e) => setProvider(e.target.value as Provider)}
      >
        {PROVIDERS.filter((p) => providers.includes(p.id)).map((p) => (
          <option key={p.id} value={p.id}>
            {p.label}
          </option>
        ))}
      </select>

      <div className="flex-1 overflow-y-auto rounded-box bg-base-100 p-4">
        {messages.map((m, i) => (
          <div
            key={i}
            className={`chat ${m.role === "user" ? "chat-end" : "chat-start"}`}
          >
            <div
              className={`chat-bubble ${
                m.role === "error" ? "chat-bubble-error" : ""
              }`}
            >
              {m.text}
            </div>
          </div>
        ))}
        {busy && <div className="chat chat-start"><div className="chat-bubble"><span className="loading loading-dots" /></div></div>}
      </div>

      <div className="flex gap-2">
        <input
          className="input input-bordered flex-1"
          placeholder="Ask something…"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && send()}
        />
        <button className="btn btn-primary" disabled={busy} onClick={send}>
          Send
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Rewrite the home route to compose Settings + Chat**

Replace the entire contents of `apps/desktop/src/routes/index.tsx` with:

```tsx
import { useEffect, useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { listConfiguredProviders, type Provider } from "../lib/api";
import { Settings } from "../components/Settings";
import { Chat } from "../components/Chat";

export const Route = createFileRoute("/")({
  component: Home,
});

function Home() {
  const [providers, setProviders] = useState<Provider[] | null>(null);

  async function refresh() {
    setProviders(await listConfiguredProviders());
  }

  useEffect(() => {
    refresh();
  }, []);

  return (
    <main className="mx-auto flex max-w-3xl flex-col gap-6">
      <h1 className="text-2xl font-bold">Manch</h1>
      {providers === null ? (
        <span className="loading loading-spinner" />
      ) : providers.length === 0 ? (
        <Settings onSaved={refresh} />
      ) : (
        <>
          <Chat providers={providers} />
          <details className="collapse collapse-arrow bg-base-100">
            <summary className="collapse-title">Add another provider key</summary>
            <div className="collapse-content">
              <Settings onSaved={refresh} />
            </div>
          </details>
        </>
      )}
    </main>
  );
}
```

- [ ] **Step 3: Type-check**

```bash
pnpm --filter @manch/desktop lint
```
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/Chat.tsx apps/desktop/src/routes/index.tsx
git commit -m "feat(desktop): chat window wired to BYOK send_prompt"
```

---

## Task 8: Manual acceptance

**Files:** none (verification only).

- [ ] **Step 1: Launch the desktop app**

```bash
pnpm --filter @manch/desktop tauri dev
```
Expected: the Manch window opens showing the "Add a provider key" card (no keys saved yet).

- [ ] **Step 2: Save an Anthropic key**

In the window, select "Anthropic (Claude)", paste a real `ANTHROPIC_API_KEY`, click **Save key**. Expected: the form is replaced by the chat window with a provider dropdown.

- [ ] **Step 3: Ask the acceptance question**

Type `What is the capital of India?` and press Enter. Expected: a user bubble appears, a loading indicator shows, then an assistant bubble answers **New Delhi**.

- [ ] **Step 4: Verify the error path**

(Optional) Stop the app, delete the saved key by removing the DB file (`~/.local/share/hq.manch.desktop/manch.sqlite3` on Linux), relaunch — the Settings card should reappear. Saving a deliberately invalid key and prompting should surface a red error bubble (not a crash).

- [ ] **Step 5: Record the result**

If the answer is correct, the slice is done. If the Gemini path is also wanted, save a Gemini key and confirm — if the model id `gemini-2.5-flash` 404s, update `GEMINI_MODEL` in `agent.rs` to a current Gemini model and rebuild.

---

## Self-Review

**Spec coverage:**
- SQLite key store → Task 2. ✅
- Settings UI (save key) → Task 6; `save_api_key` / `list_configured_providers` → Task 4. ✅
- Chat UI rendering messages + provider picker → Task 7. ✅
- `send_prompt` command calling rig (BYOK, non-streaming) → Tasks 3 + 4. ✅
- Keys user-provided & stored in SQLite (not env) → Tasks 2, 4, 6. ✅
- Manual acceptance "capital of India" → Task 8. ✅
- Out of scope (server, ACP, tools, streaming, encryption) → correctly absent.

**Placeholder scan:** No TBD/TODO. The only deferred specifics are dependency versions (resolved by `cargo add`, a real command) and the Gemini model id (flagged with a fix step in Task 8). Anthropic model id is concrete (`claude-opus-4-8`).

**Type consistency:** `Db` methods (`open`, `open_in_memory`, `save_key`, `get_key`, `list_providers`) are named identically across `db.rs`, `commands.rs`. `Provider::from_id` used consistently in `agent.rs` + `commands.rs`. Command names (`save_api_key`, `list_configured_providers`, `send_prompt`) match between `commands.rs`, `generate_handler!` in `lib.rs`, and `api.ts`. JS camelCase args (`apiKey`) map to Rust snake_case (`api_key`) via Tauri's default conversion.

**Risk note:** rig's provider API is taken from current docs; if `cargo add` resolves a version with a changed surface, Task 3's `complete` is the only place to adjust (it's isolated behind the command boundary, per the spec).
