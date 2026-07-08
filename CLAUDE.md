# CLAUDE.md

This project's agent guidance is shared across tools and lives in [`AGENTS.md`](AGENTS.md) — setup, the `just` command reference, the repository map, and the conventions the hooks/CI enforce. Read it first.

@AGENTS.md

## Notes specific to Claude Code

- Prefer the `just` recipes (`just ci`, `just test`, `just gen`) over raw `cargo`/`pnpm` so you run exactly what CI runs.
- The git hooks (Lefthook) run `fmt-check` + `lint` on commit and `clippy` + tests on push — let them run.
- Architecture and the "why" live in [`README.md`](README.md).
