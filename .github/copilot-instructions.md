# Copilot instructions

This repository's AI/agent guidance is tool-agnostic and lives in [`AGENTS.md`](../AGENTS.md). Follow it for setup, the `just` command reference, the repository map, and the conventions the git hooks and CI enforce.

Key reminders:

- Use the `just` recipes (`just ci`, `just test`, `just gen`) rather than raw `cargo`/`pnpm` — they run exactly what CI runs.
- Building `manch-server` needs `protoc`; building the Tauri desktop app on Linux needs the GTK/WebKit dev libraries (see `AGENTS.md`).
- Run `just gen` after changing `proto/` (the generated `packages/api/src/gen/` is gitignored).
- Use Conventional Commit messages; keep `clippy` (warnings = errors) and `rustfmt` clean.
