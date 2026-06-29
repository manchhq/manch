# AGENTS.md

Guidance for AI coding agents (Claude Code, Codex, Cursor, Gemini CLI, Copilot, Aider, Zed, …) working in this repository. This is the canonical, tool-agnostic agent guide — other tool-specific files (`CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md`) just point here.

For **what Manch is and why** (architecture, design stance, the extension points), read [`README.md`](README.md). This file is the **operational** guide: how to build, test, and contribute without breaking things.

## What this repo is

A polyglot monorepo for **Manch**, a domain-free substrate for building agents on top of the [Agent Client Protocol (ACP)](https://agentclientprotocol.com).

- **Rust** (Cargo workspace) — the core and backends.
- **TypeScript** (pnpm + turbo) — the desktop frontend and shared UI/API packages.

## Prerequisites

- **Rust** ≥ 1.88 (edition 2024) with `rustfmt` and `clippy`.
- **Node** ≥ 20 and **pnpm** `9.15.0` (`corepack enable` or install pnpm directly).
- **protoc** (Protocol Buffers compiler) — **required** to build `manch-server`; its `build.rs` invokes `protoc` to compile `proto/`. Install via your package manager (`apt install protobuf-compiler`, `brew install protobuf`, …).
- **Linux only:** GTK/WebKit dev libraries to build the Tauri desktop app — `libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev patchelf`. (macOS/Windows pull these from the system SDK.)

## First-time setup

```bash
just setup     # pnpm install + generate proto bindings + install git hooks
```

If you don't have [`just`](https://github.com/casey/just), install it (`cargo install just`, `brew install just`, `apt install just`) — it is the task runner and the single source of truth for every check. Run `just --list` to see all recipes.

## Everyday commands

Always prefer the `just` recipes over raw `cargo`/`pnpm` so you run exactly what CI runs.

| Task | Command |
|------|---------|
| Run **everything CI runs** (do this before pushing) | `just ci` |
| All tests (Rust + JS) | `just test` |
| Rust tests only | `just test-rust` |
| JS/TS tests only | `just test-js` |
| Protocol property tests | `just test-proptest` |
| Format Rust | `just fmt` (check-only: `just fmt-check`) |
| Lint Rust (clippy, warnings = errors) | `just clippy` |
| Typecheck TS | `just lint` |
| Regenerate proto bindings | `just gen` |
| Build server / desktop | `just build-server` / `just build-desktop` |

`just ci` runs: `gen → fmt-check → clippy → test-rust → lint → test-js → build-js`. If it passes locally, CI should pass.

## Repository map

| Path | Crate / package | What it is |
|------|-----------------|------------|
| `crates/manch-protocol` | `manch-protocol` (published lib) | The four trait contracts — `Agent`, `Tool`, `Channel`, `MemoryStore` — plus re-exported ACP vocabulary. The stable wire contract. |
| `apps/server` | `manch-server` | Optional self-hostable server exposing the core over ConnectRPC (Axum). Has a `build.rs` that compiles `proto/` (needs `protoc`). Docker-only delivery. |
| `apps/desktop/src-tauri` | `manch-desktop` | The Tauri (Rust) desktop shell. Needs GTK/WebKit on Linux. |
| `apps/desktop` | `@manch/desktop` | Desktop frontend — Vite + React 19. |
| `packages/ui` | `@manch/ui` | Shared React components (Vitest tests). |
| `packages/api` | `@manch/api` | Generated ConnectRPC TS client. **`src/gen/` is generated and gitignored** — run `just gen`. |
| `proto/` | — | Protobuf service/message definitions (source of truth for the API). |
| `docs/superpowers/` | — | Design specs and implementation plans (history of decisions). |

## Conventions that the tooling enforces

- **Git hooks (Lefthook), installed by `just setup`:**
  - `pre-commit` → `just fmt-check` + `just lint` (must be formatted and typecheck-clean).
  - `pre-push` → `just clippy` + `just test` + `just build-js`.
  - Don't bypass with `--no-verify` unless you know why.
- **Conventional Commits** — subjects like `feat:`, `fix:`, `chore:`, `ci:`, `build:`, `test:`, `docs:`. `release-plz` derives versions and changelogs from them, so they matter.
- **Clippy is `-D warnings`** and **rustfmt is enforced** — keep both clean.
- **After editing `proto/`** (or before touching TS that imports `@manch/api`), run `just gen`.
- **`Cargo.lock` is committed** — keep it in sync; the Docker/CI builds use `--locked`.

## The one architectural rule

Manch **speaks ACP's vocabulary; it does not reinvent it.** Content blocks, tool-call reporting, stop reasons, and session updates come from the `agent_client_protocol` crate, re-exported via `manch_protocol::acp`. Do **not** define parallel content/event enums. The single deliberate divergence (host-registered `Tool`s on the BYOK path) is documented in `crates/manch-protocol/src/lib.rs` and `README.md` — read those before changing the protocol crate.

## Versioning & releases (don't hand-edit)

- `manch-protocol` is versioned **independently** and published to crates.io by `release-plz` on merge to `main`.
- `manch-server` and `manch-desktop` are products (`publish = false`); they ship on a `v*` git tag — desktop installers via `tauri-action`, server as a Docker image on GHCR (`ghcr.io/manchhq/manch-server`).
- Let `release-plz` manage version bumps; don't bump crate versions by hand.

## Optional: AI code-navigation tools

These are **optional** — the project builds, tests, and ships entirely without them. They help AI agents (and humans) navigate the codebase structurally instead of grepping. `just`/CI remain the source of truth for correctness.

### Serena (configured here)

[Serena](https://github.com/oraios/serena) gives symbol-level navigation (find a symbol, its references, callers) and project memories, without reading whole files. This repo is already set up for it:

- `.serena/project.yml` — project config (languages: **rust**, **typescript**) and `.serena/memories/` — onboarding notes. Both are committed.
- `.mcp.json` — declares the Serena MCP server, so any MCP-aware tool (Claude Code, etc.) opening this repo can start it.

To use it you need:

- **[uv](https://docs.astral.sh/uv/)** (provides `uvx`, which launches Serena) — `curl -LsSf https://astral.sh/uv/install.sh | sh`.
- For Rust symbol navigation, **rust-analyzer** — `rustup component add rust-analyzer`.

In Claude Code, opening the repo will prompt to approve the `serena` server from `.mcp.json`. Other clients: point them at the command in `.mcp.json` (`uvx --from git+https://github.com/oraios/serena serena start-mcp-server --context ide-assistant --project .`).

### code-review-graph (configured here)

[code-review-graph](https://code-review-graph.com) maintains a persistent, incremental knowledge graph of the codebase for token-efficient, context-aware reviews — change/impact analysis, semantic symbol search, and test-coverage queries. It's declared in `.mcp.json` (started as `code-review-graph serve`).

To use it:

- Install the CLI: `pip install code-review-graph` (or `pipx install code-review-graph`).
- Build the graph once for this repo: `code-review-graph build`. Keep it current with `code-review-graph update` (incremental) or `code-review-graph watch`; inspect it with `code-review-graph status`.
- The graph is stored **outside the repo** (`~/.code-review-graph/`), so there's nothing to commit or gitignore.

Your MCP client then starts the `code-review-graph` server from `.mcp.json` and can use its graph tools (change detection, impact radius, semantic node search, …).

## When you're done with a change

1. `just ci` is green.
2. Commit with a Conventional Commit message.
3. If you changed the protocol crate, the proto, or public APIs, say so in the PR — those have the widest blast radius.
