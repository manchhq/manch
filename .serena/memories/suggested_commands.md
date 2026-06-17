# Commands & conventions (via `just`)

The `justfile` is the single source of truth; prefer recipes over raw cargo/pnpm so you match CI.

- `just setup` — pnpm install + `just gen` + install git hooks (first-time).
- `just ci` — everything CI runs: gen → fmt-check → clippy → test-rust → lint → test-js → build-js. Run before pushing.
- `just test` / `just test-rust` / `just test-js` / `just test-proptest`.
- `just fmt` (check: `just fmt-check`), `just clippy` (warnings = errors), `just lint` (TS typecheck).
- `just gen` — regenerate proto TS bindings (after editing `proto/` or before TS work; `packages/api/src/gen/` is gitignored).
- `just build-server` / `just build-desktop`.
- `just --list` — all recipes.

## Conventions the tooling enforces
- Git hooks (Lefthook): pre-commit = fmt-check + lint; pre-push = clippy + test + build-js.
- Conventional Commit subjects (`feat:`/`fix:`/`chore:`/`ci:`/`build:`/`test:`/`docs:`) — release-plz derives versions from them.
- `Cargo.lock` is committed; Docker/CI build `--locked`.
- Build prerequisites: `protoc` (for `manch-server`), and on Linux GTK/WebKit dev libs for the Tauri desktop crate. Serena's Rust LSP needs `rust-analyzer` (`rustup component add rust-analyzer`).

## Versioning / releases (don't hand-edit)
- `manch-protocol` versioned independently, published to crates.io by release-plz on merge to `main`.
- `manch-server` + `manch-desktop` are `publish = false`; ship on a `v*` tag (installers + GHCR Docker image).
