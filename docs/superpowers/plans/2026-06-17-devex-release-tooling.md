# Devex & Release Tooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the non-code infrastructure Manch needs to develop and ship: Lefthook git hooks, an expanded justfile as the single source of truth for checks, proptest serde round-trips on `manch-protocol`, release-plz crate publishing, and CI + tag-driven desktop/server-Docker release workflows.

**Architecture:** Hooks and CI both call `just` recipes, so check logic lives in exactly one place. Versioning splits into two tracks — `manch-protocol` is an independently-versioned library published to crates.io by release-plz; the desktop app and server are products released on a `v*` git tag (desktop installers via `tauri-action`, server as a GHCR Docker image). No product behavior changes except making two protocol structs serializable.

**Tech Stack:** Rust (Cargo workspace, edition 2024, rust 1.85), pnpm + turbo (JS), Lefthook, just, proptest, release-plz, GitHub Actions, Docker/GHCR, tauri-action.

## Global Constraints

- Conventional-commit subjects on every commit (release-plz derives versions from them): `feat:`, `fix:`, `chore:`, `ci:`, `build:`, `test:`, `docs:`.
- End every commit message with the trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Work happens on branch `chore/devex-release-tooling` (already checked out).
- Rust: edition 2024, `rust-version = 1.85`. License `MIT OR Apache-2.0`.
- pnpm version `9.15.0`; Node 22 in CI.
- Repo secret `CARGO_REGISTRY_TOKEN` already exists. GHCR pushes use the built-in `GITHUB_TOKEN` (no extra secret).
- Server ships as a Docker image only (GHCR now, Docker Hub later) — no native server binaries, no win/mac server targets.
- The JS build chain depends on generated proto code (`packages/api/src/gen/`, gitignored). Always run `pnpm generate` (alias `just gen`) before any JS lint/test/build.
- GitHub Action version pins to use verbatim: `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `pnpm/action-setup@v4`, `actions/setup-node@v4`, `taiki-e/install-action@just`, `release-plz/action@v0.5`, `tauri-apps/tauri-action@v0`, `docker/setup-buildx-action@v3`, `docker/login-action@v3`, `docker/metadata-action@v5`, `docker/build-push-action@v6`.

---

### Task 1: Justfile — the single source of truth for tasks

**Files:**
- Modify: `justfile` (currently only `default` + `sweep`)

**Interfaces:**
- Produces (recipes other tasks/hooks/CI call): `fmt`, `fmt-check`, `clippy`, `check`, `lint`, `gen`, `test`, `test-rust`, `test-js`, `test-proptest`, `build`, `build-js`, `build-server`, `build-desktop`, `ci`, `setup`, `sweep`.

- [ ] **Step 1: Replace the justfile with the full recipe set**

Write `justfile` (repo root) with exactly this content:

```just
# Manch task runner. Run `just` to list recipes.

# List available recipes
default:
    @just --list

# ── Setup ─────────────────────────────────────────────
# First-time setup: install JS deps, generate proto, install git hooks
setup:
    pnpm install
    just gen
    pnpm exec lefthook install

# Generate protobuf TS bindings (buf) into packages/api/src/gen
gen:
    pnpm generate

# ── Quality ───────────────────────────────────────────
# Format all Rust code
fmt:
    cargo fmt --all

# Verify Rust formatting without writing (used by hooks/CI)
fmt-check:
    cargo fmt --all -- --check

# Lint Rust with clippy; warnings are errors
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Cheap compile check of the whole workspace
check:
    cargo check --workspace

# Typecheck JS/TS across the workspace
lint:
    pnpm turbo run lint

# ── Test ──────────────────────────────────────────────
# Run all tests (Rust + JS)
test: test-rust test-js

# Run the Rust workspace test suite (includes proptest)
test-rust:
    cargo test --workspace

# Run JS/TS tests
test-js:
    pnpm turbo run test

# Run only the manch-protocol property tests (override count via PROPTEST_CASES)
test-proptest:
    cargo test -p manch-protocol --test serde_roundtrip

# ── Build ─────────────────────────────────────────────
# Build server binary (release) + JS
build: build-server build-js

# Build JS packages/apps
build-js:
    pnpm turbo run build

# Build the server binary in release mode
build-server:
    cargo build --release -p manch-server

# Build the desktop app bundles (installers)
build-desktop:
    pnpm --filter @manch/desktop tauri build

# ── Aggregate ─────────────────────────────────────────
# Run exactly what GitHub CI runs
ci: gen fmt-check clippy test-rust lint test-js build-js
    @echo "✓ CI checks passed"

# ── Maintenance ───────────────────────────────────────
# Remove Cargo build artifacts not accessed in the last day (needs cargo-sweep)
sweep:
    cargo sweep --time 1
```

- [ ] **Step 2: Verify recipes list cleanly**

Run: `just --list`
Expected: all recipes above appear with their doc comments, no parse error.

- [ ] **Step 3: Verify a representative Rust recipe runs**

Run: `just fmt-check`
Expected: exits 0 (repo is currently formatted) — or reports specific files if not; if it reports files, run `just fmt` and re-run.

- [ ] **Step 4: Commit**

```bash
git add justfile
git commit -m "build: expand justfile into the canonical task runner

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Lefthook git hooks

**Files:**
- Create: `lefthook.yml`
- Modify: `package.json` (root — add `prepare` script + `lefthook` devDependency)

**Interfaces:**
- Consumes (from Task 1): `just fmt-check`, `just lint`, `just clippy`, `just test`, `just build-js`.

- [ ] **Step 1: Add lefthook as a root dev dependency**

Run: `pnpm add -D -w lefthook`
Expected: `lefthook` added under `devDependencies` in root `package.json`, lockfile updated.

- [ ] **Step 2: Add the `prepare` script so hooks install on `pnpm install`**

In root `package.json`, add to the `"scripts"` object (keep existing scripts):

```json
"prepare": "lefthook install"
```

- [ ] **Step 3: Create `lefthook.yml`**

Write `lefthook.yml` (repo root) with this content:

```yaml
# Hooks delegate to `just` recipes so logic lives in one place (see justfile).
pre-commit:
  parallel: true
  commands:
    fmt:
      run: just fmt-check
    lint:
      run: just lint

pre-push:
  parallel: true
  commands:
    clippy:
      run: just clippy
    test:
      run: just test
    build-js:
      run: just build-js
```

- [ ] **Step 4: Install and verify hooks are wired**

Run: `pnpm install && pnpm exec lefthook install`
Expected: output reports hooks `pre-commit` and `pre-push` synced; `.git/hooks/pre-commit` and `.git/hooks/pre-push` now exist.

- [ ] **Step 5: Dry-run the pre-commit hook**

Run: `pnpm exec lefthook run pre-commit`
Expected: `fmt` and `lint` commands run and pass (assumes `just gen` has been run at least once so TS typecheck has generated code; if `lint` fails on missing `src/gen`, run `just gen` first, then re-run).

- [ ] **Step 6: Commit**

```bash
git add lefthook.yml package.json pnpm-lock.yaml
git commit -m "build: add Lefthook pre-commit/pre-push hooks via just recipes

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: proptest serde round-trips on `manch-protocol`

Make `ToolSchema` and `Context` serializable and prove a JSON round-trip is lossless. The crate exists to be a stable wire contract; this is its core invariant.

**Files:**
- Modify: `crates/manch-protocol/src/lib.rs` (add serde + PartialEq derives + import)
- Modify: `crates/manch-protocol/Cargo.toml` (add `proptest` dev-dependency)
- Create: `crates/manch-protocol/tests/serde_roundtrip.rs`

**Interfaces:**
- Produces: `ToolSchema` and `Context` now implement `serde::Serialize`, `serde::Deserialize`, and `PartialEq` (in addition to existing `Debug, Clone`).
- Consumes (existing ACP types, all already `Serialize + Deserialize + PartialEq`): `agent_client_protocol::schema::{ContentBlock, TextContent, ToolKind}`, with `TextContent::new(impl Into<String>)` and `ContentBlock::Text(TextContent)`.

- [ ] **Step 1: Write the failing property tests**

Create `crates/manch-protocol/tests/serde_roundtrip.rs`:

```rust
//! Property tests: the protocol's own serializable types must survive a JSON
//! round-trip unchanged. A stable wire contract is the reason this crate exists.

use agent_client_protocol::schema::{ContentBlock, TextContent, ToolKind};
use manch_protocol::{Context, ToolSchema};
use proptest::prelude::*;

/// Arbitrary JSON values, deliberately excluding floats so equality is exact
/// (no NaN, no precision loss). Integers, strings, bools, null, nested arrays
/// and objects only.
fn arb_json() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(|n| serde_json::Value::Number(n.into())),
        any::<String>().prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(3, 16, 5, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(serde_json::Value::Array),
            prop::collection::vec((any::<String>(), inner), 0..5)
                .prop_map(|kvs| serde_json::Value::Object(kvs.into_iter().collect())),
        ]
    })
}

fn arb_tool_kind() -> impl Strategy<Value = ToolKind> {
    prop_oneof![
        Just(ToolKind::Read),
        Just(ToolKind::Edit),
        Just(ToolKind::Delete),
        Just(ToolKind::Move),
        Just(ToolKind::Search),
        Just(ToolKind::Execute),
        Just(ToolKind::Think),
        Just(ToolKind::Fetch),
        Just(ToolKind::SwitchMode),
        Just(ToolKind::Other),
    ]
}

fn arb_tool_schema() -> impl Strategy<Value = ToolSchema> {
    (any::<String>(), any::<String>(), arb_tool_kind(), arb_json()).prop_map(
        |(name, description, kind, input_schema)| ToolSchema {
            name,
            description,
            kind,
            input_schema,
        },
    )
}

fn arb_context() -> impl Strategy<Value = Context> {
    (any::<String>(), prop::collection::vec(any::<String>(), 0..5)).prop_map(
        |(session_id, texts)| Context {
            session_id,
            blocks: texts
                .into_iter()
                .map(|t| ContentBlock::Text(TextContent::new(t)))
                .collect(),
        },
    )
}

proptest! {
    #[test]
    fn tool_schema_json_roundtrip(schema in arb_tool_schema()) {
        let json = serde_json::to_string(&schema).unwrap();
        let back: ToolSchema = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(schema, back);
    }

    #[test]
    fn context_json_roundtrip(ctx in arb_context()) {
        let json = serde_json::to_string(&ctx).unwrap();
        let back: Context = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(ctx, back);
    }
}
```

- [ ] **Step 2: Add the proptest dev-dependency**

In `crates/manch-protocol/Cargo.toml`, append after the `[dependencies]` block:

```toml
[dev-dependencies]
proptest = "1"
```

- [ ] **Step 3: Run the tests to verify they FAIL to compile**

Run: `cargo test -p manch-protocol --test serde_roundtrip`
Expected: compile error — `ToolSchema`/`Context` do not implement `Serialize`/`Deserialize`/`PartialEq` (e.g. "the trait bound `ToolSchema: Serialize` is not satisfied").

- [ ] **Step 4: Add the serde + PartialEq derives**

In `crates/manch-protocol/src/lib.rs`, add this import near the other top-level `use` statements (after `use async_trait::async_trait;`):

```rust
use serde::{Deserialize, Serialize};
```

Change the `Context` derive line from:

```rust
#[derive(Debug, Clone)]
pub struct Context {
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Context {
```

Change the `ToolSchema` derive line from:

```rust
#[derive(Debug, Clone)]
pub struct ToolSchema {
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSchema {
```

- [ ] **Step 5: Run the tests to verify they PASS**

Run: `cargo test -p manch-protocol --test serde_roundtrip`
Expected: PASS — `tool_schema_json_roundtrip ... ok` and `context_json_roundtrip ... ok`.

- [ ] **Step 6: Verify the recipe and a higher case count**

Run: `PROPTEST_CASES=2048 just test-proptest`
Expected: PASS at 2048 cases each.

- [ ] **Step 7: Verify the workspace still builds and clippy is clean**

Run: `just clippy`
Expected: exits 0, no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/manch-protocol/src/lib.rs crates/manch-protocol/Cargo.toml crates/manch-protocol/tests/serde_roundtrip.rs Cargo.lock
git commit -m "test: serde JSON round-trip property tests for protocol contract

Make ToolSchema and Context serde-serializable and assert lossless JSON
round-trips with proptest.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Versioning split + publish flags

Decouple `manch-protocol`'s version from the workspace (release-plz owns it) and mark the two app crates as non-publishable.

**Files:**
- Modify: `crates/manch-protocol/Cargo.toml` (independent `version = "0.0.1"`)
- Modify: `Cargo.toml` (root — bump the internal `manch-protocol` dep requirement to `0.0.1`)
- Modify: `apps/server/Cargo.toml` (`publish = false`)
- Modify: `apps/desktop/src-tauri/Cargo.toml` (`publish = false`)

**Interfaces:**
- Consumes (from Task 3): the modified `crates/manch-protocol/Cargo.toml`.
- Produces: `manch-protocol` at version `0.0.1`; `manch-server` and `manch-desktop` with `publish = false`.

- [ ] **Step 1: Give `manch-protocol` an independent version**

In `crates/manch-protocol/Cargo.toml`, change:

```toml
version.workspace = true
```

to:

```toml
version = "0.0.1"
```

- [ ] **Step 2: Update the internal dependency requirement**

In root `Cargo.toml`, under `[workspace.dependencies]`, change:

```toml
manch-protocol = { path = "crates/manch-protocol", version = "0.0.0" }
```

to:

```toml
manch-protocol = { path = "crates/manch-protocol", version = "0.0.1" }
```

- [ ] **Step 3: Mark the server crate non-publishable**

In `apps/server/Cargo.toml`, add `publish = false` to the `[package]` table (after the `rust-version.workspace = true` line):

```toml
publish = false
```

- [ ] **Step 4: Mark the desktop crate non-publishable**

In `apps/desktop/src-tauri/Cargo.toml`, add `publish = false` to the `[package]` table (after the `rust-version.workspace = true` line):

```toml
publish = false
```

- [ ] **Step 5: Verify metadata + build resolve**

Run: `cargo metadata --format-version 1 --no-deps > /dev/null && cargo check --workspace`
Expected: exits 0 — version requirement `0.0.1` satisfied across the workspace, no resolution error.

- [ ] **Step 6: Confirm the publish flags are visible to Cargo**

Run: `cargo publish -p manch-server --dry-run 2>&1 | head -5 || true`
Expected: Cargo refuses because the crate is marked `publish = false` (message naming `publish = false`), confirming the flag took effect.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/manch-protocol/Cargo.toml apps/server/Cargo.toml apps/desktop/src-tauri/Cargo.toml
git commit -m "build: version manch-protocol independently at 0.0.1; mark apps no-publish

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: release-plz — crate publishing

Automate `manch-protocol` versioning, changelog, and crates.io publishing; exclude the app crates.

**Files:**
- Create: `release-plz.toml`
- Create: `.github/workflows/release-plz.yml`

**Interfaces:**
- Consumes (from Task 4): `manch-protocol` publishable at `0.0.1`; apps `publish = false`.

- [ ] **Step 1: Create `release-plz.toml`**

Write `release-plz.toml` (repo root):

```toml
# Only the library crate is published to crates.io. The desktop app and server
# are products, released on a `v*` git tag (see .github/workflows/release.yml).

[[package]]
name = "manch-protocol"
publish = true
release = true

[[package]]
name = "manch-server"
release = false

[[package]]
name = "manch-desktop"
release = false
```

- [ ] **Step 2: Create the release-plz workflow**

Write `.github/workflows/release-plz.yml`:

```yaml
name: release-plz

on:
  push:
    branches: [main]

permissions:
  contents: write
  pull-requests: write

jobs:
  release-plz:
    runs-on: ubuntu-latest
    concurrency:
      group: release-plz-${{ github.ref }}
      cancel-in-progress: false
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: dtolnay/rust-toolchain@stable
      - uses: release-plz/action@v0.5
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

- [ ] **Step 3: Validate the workflow YAML**

Run: `pnpm dlx @action-validator/cli .github/workflows/release-plz.yml || python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release-plz.yml')); print('yaml ok')"`
Expected: validation passes (or at minimum `yaml ok` — confirms well-formed YAML).

- [ ] **Step 4: Validate the release-plz config parses**

Run: `python3 -c "import tomllib; tomllib.load(open('release-plz.toml','rb')); print('toml ok')"`
Expected: `toml ok`.

- [ ] **Step 5: Commit**

```bash
git add release-plz.toml .github/workflows/release-plz.yml
git commit -m "ci: add release-plz to publish manch-protocol to crates.io

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: CI workflow

One workflow that runs `just ci` (the exact local check set) on every PR and push to `main`.

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes (from Task 1): `just ci` (= `gen fmt-check clippy test-rust lint test-js build-js`).

- [ ] **Step 1: Create the CI workflow**

Write `.github/workflows/ci.yml`:

```yaml
name: ci

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read

jobs:
  ci:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - uses: pnpm/action-setup@v4
        with:
          version: 9.15.0
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - uses: taiki-e/install-action@just
      - run: pnpm install --frozen-lockfile
      - run: just ci
```

- [ ] **Step 2: Validate the workflow YAML**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('yaml ok')"`
Expected: `yaml ok`.

- [ ] **Step 3: Reproduce CI locally to prove the recipe parity holds**

Run: `pnpm install --frozen-lockfile && just ci`
Expected: ends with `✓ CI checks passed` (gen, fmt-check, clippy, rust tests, JS lint/test, JS build all pass).

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: run just ci (fmt, clippy, tests, JS lint/test/build) on PRs and main

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Release workflow — desktop installers + server Docker image

On a `v*` tag: build desktop installers for macOS/Windows/Linux via `tauri-action`, and build+push the server Docker image to GHCR.

**Files:**
- Create: `Dockerfile` (repo root — server image)
- Create: `.dockerignore`
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: server crate `manch-server` (built with `cargo build --release -p manch-server`; `build.rs` reads `../../proto`, pure-Rust codegen, no `protoc` needed); Tauri config `beforeBuildCommand = pnpm --filter @manch/desktop build`, `frontendDist = ../dist`.

- [ ] **Step 1: Create `.dockerignore`**

Write `.dockerignore` (repo root):

```
target/
node_modules/
dist/
.turbo/
.git/
apps/desktop/src-tauri/target/
packages/api/src/gen/
storybook-static/
**/*.md
```

- [ ] **Step 2: Create the server `Dockerfile`**

Write `Dockerfile` (repo root). Multi-stage: build `manch-server` against the full workspace (so `build.rs` finds `proto/`), then a slim runtime:

```dockerfile
# syntax=docker/dockerfile:1

# ── Build stage ───────────────────────────────────────
FROM rust:1.85-bookworm AS builder
WORKDIR /build
# Copy the whole workspace: manch-server's build.rs reads ../../proto and the
# build needs the workspace Cargo.toml/Cargo.lock. No protoc required — the
# connectrpc/buffa codegen is pure Rust.
COPY . .
RUN cargo build --release -p manch-server

# ── Runtime stage ─────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/manch-server /usr/local/bin/manch-server
# Bind to all interfaces inside the container (the app defaults to 127.0.0.1).
ENV MANCH_ADDR=0.0.0.0:8787
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/manch-server"]
```

- [ ] **Step 3: Build the image locally to validate the Dockerfile**

Run: `docker build -t manch-server:plan-test .`
Expected: build succeeds through both stages; final line `naming to docker.io/library/manch-server:plan-test`.

- [ ] **Step 4: Smoke-test the image responds on /health**

Run:
```bash
docker run -d --rm -p 8787:8787 --name manch-smoke manch-server:plan-test
sleep 2
curl -fsS http://127.0.0.1:8787/health; echo
docker stop manch-smoke
```
Expected: prints `OK`.

- [ ] **Step 5: Create the release workflow**

Write `.github/workflows/release.yml`:

```yaml
name: release

on:
  push:
    tags: ["v*"]

permissions:
  contents: write

jobs:
  desktop:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-latest
            args: "--target aarch64-apple-darwin"
          - platform: macos-latest
            args: "--target x86_64-apple-darwin"
          - platform: ubuntu-22.04
            args: ""
          - platform: windows-latest
            args: ""
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4
      - name: Install Linux build deps
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libgtk-3-dev
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.platform == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}
      - uses: Swatinem/rust-cache@v2
      - uses: pnpm/action-setup@v4
        with:
          version: 9.15.0
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm generate
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          projectPath: apps/desktop
          tagName: ${{ github.ref_name }}
          releaseName: "Manch ${{ github.ref_name }}"
          releaseDraft: true
          prerelease: false
          args: ${{ matrix.args }}

  server-docker:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/metadata-action@v5
        id: meta
        with:
          images: ghcr.io/manchhq/manch-server
          tags: |
            type=semver,pattern={{version}}
            type=raw,value=latest
      - uses: docker/build-push-action@v6
        with:
          context: .
          file: ./Dockerfile
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

- [ ] **Step 6: Validate the workflow YAML**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml')); print('yaml ok')"`
Expected: `yaml ok`.

- [ ] **Step 7: Commit**

```bash
git add Dockerfile .dockerignore .github/workflows/release.yml
git commit -m "ci: release workflow — tauri installers + server Docker image to GHCR

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Wire-up docs + first test deploy

Document the new commands for contributors and validate the crate track end-to-end with the `0.0.1` publish.

**Files:**
- Modify: `README.md` (add a short "Development" section if one isn't already present; otherwise append the recipes)

**Interfaces:**
- Consumes: all recipes/workflows from Tasks 1–7.

- [ ] **Step 1: Add a Development section to the README**

Add this section to `README.md` (place it near the existing build/run instructions; if a "Development" heading already exists, merge these lines into it):

```markdown
## Development

```bash
just setup     # install JS deps, generate proto bindings, install git hooks
just ci        # run everything CI runs: fmt, clippy, tests, JS lint/test/build
just test      # all tests (Rust + JS)
just --list    # see all recipes
```

Git hooks (via Lefthook) run `fmt` + `lint` on commit and `clippy` + tests on push.
`manch-protocol` is published to crates.io by release-plz on merge to `main`.
Tagging `vX.Y.Z` builds desktop installers and the server Docker image
(`ghcr.io/manchhq/manch-server`).
```

- [ ] **Step 2: Verify the README renders without broken fences**

Run: `python3 -c "import re,sys; t=open('README.md').read(); n=t.count('```'); print('fences balanced' if n%2==0 else 'UNBALANCED fences')"`
Expected: `fences balanced`.

- [ ] **Step 3: Commit the docs**

```bash
git add README.md
git commit -m "docs: document just recipes, hooks, and release flow

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 4: Open the PR**

```bash
git push -u origin chore/devex-release-tooling
gh pr create --title "Devex & release tooling: hooks, just, proptest, release-plz, CI/CD" \
  --body "Implements docs/superpowers/specs/2026-06-17-devex-release-tooling-design.md.

- Lefthook pre-commit/pre-push hooks calling just recipes
- Expanded justfile as the single source of truth
- proptest serde round-trips on manch-protocol (ToolSchema, Context)
- release-plz publishing manch-protocol to crates.io (now at 0.0.1)
- CI workflow (just ci) on PRs and main
- release workflow: tauri installers + server Docker image to GHCR

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

- [ ] **Step 5: First test deploy (after merge) — manual verification checklist**

This step is performed by a human after the PR merges to `main`; it is not automatable from this branch:

1. Confirm the `release-plz` Action run on `main` opens a "release" PR (or directly publishes) for `manch-protocol 0.0.1`.
2. Merge the release PR; confirm the `release-plz` run publishes `manch-protocol 0.0.1` to crates.io and pushes a `manch-protocol-v0.0.1` tag.
3. Verify on https://crates.io/crates/manch-protocol that `0.0.1` is live.
4. (Optional, when ready) push a `v0.0.1` tag to trigger `release.yml` and confirm desktop installers attach to a draft Release and `ghcr.io/manchhq/manch-server:0.0.1` is pushed.

---

## Self-Review

**Spec coverage:**
- Lefthook hooks → Task 2 ✓
- Expanded justfile (tests, proptest recipe) → Task 1 ✓
- proptest on manch-protocol (serde round-trip, ToolSchema+Context) → Task 3 ✓ (target adjusted per discovery + user decision: types were not serde-capable; serde added)
- release-plz independent crate versioning + crates.io publish → Tasks 4, 5 ✓
- CI workflow (fmt/clippy/test + JS lint/test/build, caching) → Task 6 ✓
- Tag-driven desktop installers via tauri-action → Task 7 ✓
- Server Docker image to GHCR (push enabled now, Docker Hub later, no native binaries) → Task 7 ✓
- First `0.0.1` test deploy → Task 8 ✓
- YAGNI exclusions (no commitlint, JS packages stay private, no server native binaries) → respected ✓

**Placeholder scan:** No TBD/TODO; every code/config step shows full content; every verification step has an exact command + expected output.

**Type consistency:** `ToolSchema { name, description, kind, input_schema }` and `Context { session_id, blocks }` match `lib.rs` exactly. `ToolKind` variants match ACP schema 0.13.6 (`Read, Edit, Delete, Move, Search, Execute, Think, Fetch, SwitchMode, Other`). `TextContent::new` + `ContentBlock::Text` match the construction used at `apps/desktop/src-tauri/src/agent.rs:234`. Recipe names referenced by hooks/CI (`fmt-check`, `lint`, `clippy`, `test`, `build-js`, `gen`, `ci`) all defined in Task 1.
