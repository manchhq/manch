# Manch — project overview

Polyglot monorepo for **Manch**, a domain-free substrate for building agents on top of the Agent Client Protocol (ACP).

- **Rust** (Cargo workspace) — the core/backends.
- **TypeScript** (pnpm + turbo) — desktop frontend + shared UI/API packages.

## Repository map
- `crates/manch-protocol` — published library: the four trait contracts (`Agent`, `Tool`, `Channel`, `MemoryStore`) + re-exported ACP vocabulary. The stable wire contract.
- `apps/server` — `manch-server`, ConnectRPC-over-Axum server. `build.rs` compiles `proto/` and needs **`protoc`**. Docker-only delivery.
- `apps/desktop/src-tauri` — `manch-desktop`, the Tauri (Rust) shell. Needs GTK/WebKit on Linux.
- `apps/desktop` — `@manch/desktop`, Vite + React 19 frontend.
- `packages/ui` — `@manch/ui`, shared React components (Vitest).
- `packages/api` — `@manch/api`, generated ConnectRPC TS client; `src/gen/` is generated (gitignored) → run `just gen`.
- `proto/` — protobuf definitions (API source of truth).

## Canonical docs (prefer these — keep them authoritative, not this memory)
- `AGENTS.md` — operational guide (commands, repo map, conventions).
- `README.md` — architecture and the "why".
- `docs/superpowers/` — design specs + implementation plans.

## The one architectural rule
Manch speaks ACP's vocabulary; it does not reinvent it. Content/event types come from `agent_client_protocol`, re-exported via `manch_protocol::acp`. Do not define parallel enums.
