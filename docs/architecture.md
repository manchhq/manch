---
governs:
  paths: ["crates/**", "apps/**", "packages/**", "proto/**", "Cargo.toml", "justfile", "lefthook.yml", ".github/workflows/**"]
  shapes: [placement, build-constraints]
  verify: source
---

# Architecture and contribution baseline

This baseline describes the checked-in workspace. README.md's architecture and
roadmap include planned components; a planned crate is not an existing placement.
Sources: [Cargo.toml](../Cargo.toml), [README.md](../README.md).

## Placement

| Change | Location | Source |
|---|---|---|
| Agent, tool, memory, channel and permission contracts; ACP vocabulary | `crates/manch-protocol` | [crate documentation](../crates/manch-protocol/src/lib.rs) |
| Provider authentication, request mapping, streaming and HTTP policy | `crates/manch-llm` | [crate manifest](../crates/manch-llm/Cargo.toml), [public module](../crates/manch-llm/src/lib.rs) |
| External ACP agent hosting | `crates/manch-acp` | [crate manifest](../crates/manch-acp/Cargo.toml) |
| Registries and host tool orchestration | `crates/manch-core` | [crate manifest](../crates/manch-core/Cargo.toml) |
| Desktop DTOs and TypeScript binding generator | `crates/manch-dto` | [crate manifest](../crates/manch-dto/Cargo.toml) |
| Optional ConnectRPC server | `apps/server` | [crate manifest](../apps/server/Cargo.toml) |
| Tauri desktop shell | `apps/desktop/src-tauri` | [crate manifest](../apps/desktop/src-tauri/Cargo.toml) |
| React desktop frontend | `apps/desktop` | [package manifest](../apps/desktop/package.json) |
| Shared React components and their tests | `packages/ui` | [package manifest](../packages/ui/package.json) |
| API exports and generated ConnectRPC client | `packages/api` | [package manifest](../packages/api/package.json), [generator config](../buf.gen.yaml) |
| Protobuf service and message source | `proto` | [generator config](../buf.gen.yaml), [server build script](../apps/server/build.rs) |

The Rust workspace declares these seven crates in [Cargo.toml](../Cargo.toml).
JavaScript packages are selected by [pnpm-workspace.yaml](../pnpm-workspace.yaml).
Dependencies belong in each unit's `Cargo.toml` or `package.json`, with shared Rust
dependencies in the workspace manifest. Sources: the manifests linked above.

## Gate commands

Use `just` recipes. [CI](../.github/workflows/ci.yml) installs dependencies with
`pnpm install --frozen-lockfile` and runs `just ci`.

| Command | Behavior | Source |
|---|---|---|
| `just setup` | Install JS dependencies, generate bindings, install hooks | [justfile](../justfile) |
| `just ci` | Run `gen`, `fmt-check`, `clippy`, `doc`, `test-rust`, `lint`, `test-js`, `build-js` | [justfile](../justfile) |
| `just gen` | Generate protobuf TS and DTO bindings | [justfile](../justfile), [buf.gen.yaml](../buf.gen.yaml), [gen-types.rs](../crates/manch-dto/src/bin/gen-types.rs) |
| `just fmt-check`, `just clippy`, `just doc` | Check Rust formatting, lint with warnings denied, build rustdoc with warnings denied | [justfile](../justfile) |
| `just test-rust`, `just test-js`, `just test` | Rust workspace tests, JS package tests, or both | [justfile](../justfile) |
| `just lint`, `just build-js` | Typecheck and build JS packages via Turbo | [justfile](../justfile), [package.json](../package.json) |

The root package scripts also expose `prepare`, `build`, `test`, `lint`,
`storybook`, `generate` and `desktop:dev`. Their target packages and configuration
are present; executable availability depends on dependency installation. Sources:
[package.json](../package.json), [UI manifest](../packages/ui/package.json),
[desktop manifest](../apps/desktop/package.json), [buf.gen.yaml](../buf.gen.yaml).

## Build constraints

- **Pre-commit hook compiles:** yes
- **Cold build cost:** 1m38s for `just ci` from an absent `target/` directory on the reference machine (2026-11-26; details below)
- **Concurrent builds safe:** not recorded

The pre-commit hook runs `just fmt-check` and `just lint` in parallel; `lint`
invokes TypeScript's compiler with `--noEmit`. It does not build Rust binaries or
emit JavaScript. Pre-push runs `just clippy`, `just test` and `just build-js` in
parallel. Sources: [lefthook.yml](../lefthook.yml), [justfile](../justfile),
[desktop manifest](../apps/desktop/package.json), [UI manifest](../packages/ui/package.json),
[API manifest](../packages/api/package.json).

The reference measurement used commit
`36c17f7e03b05dc5ede15768c44d6f27edfda823`, Rust/Cargo 1.98.0, pnpm 9.15.0,
20 available logical CPUs and approximately 30 GiB RAM on Linux. `target/` was
absent before the run; JS dependencies were installed with
`pnpm install --frozen-lockfile` first. Cargo's dependency download cache already
existed, so this measures cold local compilation and the CI checks, excluding
dependency installation and downloads not needed by the run. Sources: the
2026-11-26 environment probes (`rustc --version`, `cargo --version`,
`pnpm --version`, `nproc`, `free -h`) and [timing output](build-baseline.txt).

The measured command was `/usr/bin/time -v just ci`; it exited 0 and printed
`✓ CI checks passed`. Formatting, clippy, rustdoc, Rust tests, TS checks, JS tests
and the JS build all passed. Wall time was 97.90 seconds. GNU time reported
1,402,444 KiB maximum resident set size; this is not an aggregate peak for all
concurrent compiler processes and does not establish concurrent-build safety.
Sources: [timing output](build-baseline.txt) and the terminal output of the
2026-11-26 `just ci` run. This verifies the baseline on Rust 1.98.0, not the
declared Rust 1.88 minimum.

Concurrent-build safety: not recorded. No simultaneous-build experiment was
performed. Continue serializing agent-triggered builds. The timing is a
reference measurement, not a performance guarantee for other machines.

## Traps and deliberate boundaries

| Finding | Source |
|---|---|
| Use ACP content, updates and stop reasons. `ToolInvocation` is the documented exception for host dispatch, because ACP's reporting type has no function-name field. | [protocol documentation](../crates/manch-protocol/src/lib.rs) |
| Domain behavior belongs in consumers; the provider layer must remain usable without the runtime or application shells. | [README.md](../README.md), [provider dependencies](../crates/manch-llm/Cargo.toml) |
| `packages/api/src/gen/` and desktop `bindings.ts` are generated and ignored. Run `just gen` before using those imports. | [.gitignore](../.gitignore), [justfile](../justfile) |
| `buf.gen.yaml` invokes two local executables under `packages/api/node_modules/.bin`; they require JS dependencies first. The referenced DTO generator source exists. | [buf.gen.yaml](../buf.gen.yaml), [API manifest](../packages/api/package.json), [gen-types.rs](../crates/manch-dto/src/bin/gen-types.rs) |
| Server builds require `protoc`; Linux desktop builds require GTK/WebKit development libraries. | [AGENTS.md](../AGENTS.md), [CI install step](../.github/workflows/ci.yml) |
| The current `just ci` recipe includes `doc` as well as the checks enumerated in AGENTS.md's summary. Follow the recipe. | [justfile](../justfile), [AGENTS.md](../AGENTS.md) |
| Rust 1.88 is declared at the workspace level; declaring it is not evidence that a dependency resolution builds on that toolchain. | [Cargo.toml](../Cargo.toml) |
| `manch-llm` is unpublished (`publish = false`). | [provider manifest](../crates/manch-llm/Cargo.toml) |
| Local Superpowers plans and specs are ignored. | [.gitignore](../.gitignore) |

Additional operational traps: not recorded.
