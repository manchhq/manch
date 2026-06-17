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
