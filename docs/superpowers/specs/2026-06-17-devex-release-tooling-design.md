# Devex & Release Tooling — Design

**Date:** 2026-06-17
**Status:** Approved (pending spec review)

## Goal

Add the non-code infrastructure the Manch monorepo needs to develop and ship
safely: git hooks, a task runner that is the single source of truth for checks,
property tests for the protocol contract, automated crate releases, and CI/CD
that builds desktop installers and a server Docker image.

This is a **tooling pass** — it adds no product code. The one explicit
out-of-scope note: making the server actually serve a web front-end is app code,
not covered here (the server Docker image is structured so a frontend stage can
be added later).

## Repository context

Polyglot monorepo:

- **JS side** — pnpm + turbo. `packages/ui` (`@manch/ui`, vitest tests already
  present), `packages/api` (`@manch/api`), `apps/desktop` (`@manch/desktop`,
  Vite + React 19 front-end for the Tauri app).
- **Rust side** — Cargo workspace. `crates/manch-protocol` (publishable library —
  the trait contracts), `apps/server` (`manch-server`, ConnectRPC binary),
  `apps/desktop/src-tauri` (`manch-desktop`, the Tauri app).

Current state: version is `0.0.0` everywhere via `version.workspace = true`; no
`.github/`; `justfile` only has a `sweep` recipe. Tests exist in both ecosystems.
`CARGO_REGISTRY_TOKEN` is already configured as a repo secret.

## Decisions (locked with the user)

| Area | Choice |
|------|--------|
| Git hooks | **Lefthook** (polyglot, parallel, hooks call `just` recipes) |
| Versioning | **release-plz** — independent versioning for `manch-protocol` |
| Property tests | **proptest now** on `manch-protocol` |
| Pipeline scope | **Full**, including crates.io publish; first target `0.0.1` |
| Server delivery | **Docker image only** — no native binaries, no win/mac targets |

## 1. Git hooks — Lefthook

Single root `lefthook.yml`. Hooks invoke `just` recipes so logic is not
duplicated between hooks and CI.

- **pre-commit** (fast): `just fmt-check`, `just lint` (JS typecheck). Parallel.
- **pre-push** (heavier): `just clippy`, `just test`, `just build` (JS).

Install path: a `"prepare": "lefthook install"` script in the root
`package.json`, plus `lefthook` as a root devDependency, so `pnpm install` wires
hooks automatically for every contributor. No global binary required.

## 2. Justfile — single source of truth for tasks

Expand the root `justfile`. Recipes are the same commands humans, hooks, and CI
all call.

- **Test:** `test` (all), `test-rust`, `test-js`, `test-proptest`
  (`PROPTEST_CASES` overridable via env)
- **Quality:** `fmt`, `fmt-check`, `clippy`, `lint` (JS typecheck), `check`
- **Build:** `build`, `build-desktop`, `build-server`
- **Aggregate:** `ci` — runs exactly what `ci.yml` runs, for local reproduction
- Keep existing: `sweep`

## 3. Property tests — proptest on `manch-protocol`

Add `proptest` as a dev-dependency to `crates/manch-protocol`. Property tests
assert **serde JSON round-trip stability** — `from_str(to_string(v)) == v` — on
the protocol crate's **own** serializable types (its `Error` shape and any
message / tool-argument structs it defines). The re-exported ACP types are
**not** covered here; they belong to the `agent_client_protocol` crate. The
invariant matters because `manch-protocol` exists to be a stable wire contract.

Run via `just test-proptest`.

## 4. Versioning — two independent tracks

The current `version.workspace = true = "0.0.0"` couples everything. Split:

- **Crate track (`manch-protocol`) → release-plz.** Give `manch-protocol` its own
  `version` key (decoupled from the workspace default). On push to `main`,
  release-plz opens a Release PR (version bump + changelog from conventional
  commits); merging it tags `manch-protocol-v*` and publishes to crates.io with
  `CARGO_REGISTRY_TOKEN`. `manch-server` and `manch-desktop` are marked
  `publish = false` and excluded from release-plz (`release = false`) so only the
  library publishes.
- **App track (desktop + server) → tag-driven.** Pushing a `v*` git tag triggers
  `release.yml`. The desktop version is sourced from `tauri.conf.json`.

## 5. GitHub Actions — three workflows

### `ci.yml` — on PR and push to `main`
- Rust: `fmt-check`, `clippy -D warnings`, `cargo test` (workspace).
- JS: `turbo lint`, `turbo test`, `turbo build`.
- Caching: `Swatinem/rust-cache` + pnpm store cache.
- This is the gate the local hooks mirror.

### `release-plz.yml` — on push to `main`
- Runs `release-plz` → opens/refreshes the Release PR and, on merge, publishes
  `manch-protocol` to crates.io. Uses `CARGO_REGISTRY_TOKEN` + `GITHUB_TOKEN`.

### `release.yml` — on `v*` tag
- **Desktop:** `tauri-action` build matrix — macOS, Windows, Ubuntu — producing
  installers attached to a GitHub Release.
- **Server:** build a **Docker image** from a new multi-stage `Dockerfile`
  (`cargo build --release -p manch-server` → slim Debian/distroless runtime).
  - Default registry: **GHCR** (`ghcr.io`), which pushes with the built-in
    `GITHUB_TOKEN`.
  - The **push step is gated** on the registry credential being available; until
    the user adds their key, the job builds the image (validating the Dockerfile)
    and skips the push. Registry is configurable if the user prefers Docker Hub
    (would add a `DOCKERHUB_TOKEN`).
  - **No native server binaries**, no Windows/macOS server targets.
  - The Dockerfile is structured to allow adding a front-end build stage later;
    wiring the server to serve static assets is app code and out of scope here.

## First milestone — the `0.0.1` test deploy

Set `manch-protocol` to `0.0.1` and let the crate track publish it to crates.io,
validating release-plz + the token end-to-end. App-track installers and the
server image follow on the first `v*` tag.

## Out of scope (YAGNI)

- Commit-message linting / commitlint. release-plz reads conventional commits but
  tolerates non-conventional ones; can be added later.
- Server native binaries and musl/cross targets — Docker only.
- Publishing the JS packages — they stay `private`/internal.
- Server serving a web front-end — app code, not tooling.

## Affected / new files

- New: `lefthook.yml`, `release-plz.toml`, `Dockerfile` (server),
  `.github/workflows/ci.yml`, `.github/workflows/release-plz.yml`,
  `.github/workflows/release.yml`, `.dockerignore`.
- Modified: root `justfile` (recipes), root `package.json` (`prepare` +
  `lefthook` devDep), `Cargo.toml` (workspace), `crates/manch-protocol/Cargo.toml`
  (independent `version`, `proptest` dev-dep), `apps/server/Cargo.toml` +
  `apps/desktop/src-tauri/Cargo.toml` (`publish = false`),
  `crates/manch-protocol/src/lib.rs` or a new `tests/` file (proptest tests).
