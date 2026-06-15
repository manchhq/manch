# UI + Server + Polyglot Monorepo Scaffold — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Manch's Rust-only Cargo workspace into a polyglot monorepo with a shared dumb-component React UI, a Tauri desktop shell, and a Rust ConnectRPC server, proving an end-to-end "show the server version in the app" path.

**Architecture:** A pnpm workspace + Turborepo coexist with the existing Cargo workspace at the repo root. A single `proto/` directory is the wire contract: the Rust server generates from it via `connectrpc-build` (build.rs), the TS `@manch/api` package generates from it via `buf generate`. `@manch/ui` holds presentational components (props/callbacks only); `apps/desktop` (Tauri) is the smart parent that owns routing/query and feeds data into `@manch/ui`.

**Tech Stack:** Rust (`connectrpc` 0.7 + `axum` + `tokio`), Tauri 2, pnpm + Turborepo, Vite + React 19, TanStack Router (file-based) + Query, Connect-Query, Remeda, Valibot, react-hookz, Tailwind CSS v4 + DaisyUI 5, Vitest + React Testing Library, Storybook 8, buf.

---

## File Structure

**Root (new):**
- `LICENSE-MIT`, `LICENSE-APACHE` — dual license texts.
- `package.json` — pnpm root, devDeps + turbo scripts.
- `pnpm-workspace.yaml` — globs `apps/*`, `packages/*`.
- `turbo.json` — task pipeline.
- `.npmrc` — pnpm settings.
- `buf.yaml`, `buf.gen.yaml` — proto lint + TS codegen.
- `proto/manch/v1/manch.proto` — scaffold service.
- `.gitignore` — extend with node/turbo/codegen ignores + routeTree exception.

**Root (modified):**
- `Cargo.toml` — add `crates/manch-server` and `apps/desktop/src-tauri` members.
- `README.md` — remove License section.

**`crates/manch-server/` (new):** `Cargo.toml`, `build.rs`, `src/lib.rs`, `src/service.rs`, `src/main.rs`.

**`packages/ui/` (new):** `package.json`, `tsconfig.json`, `vite.config.ts`, `vitest.config.ts`, `src/styles.css`, `src/index.ts`, `src/Button.tsx`, `src/Button.test.tsx`, `src/Button.stories.tsx`, `.storybook/main.ts`, `.storybook/preview.ts`, `src/test-setup.ts`.

**`packages/api/` (new):** `package.json`, `tsconfig.json`, `buf-managed` gen output (gitignored), `src/index.ts`, `src/transport.ts`.

**`apps/desktop/` (new):** `package.json`, `tsconfig.json`, `vite.config.ts`, `index.html`, `src/main.tsx`, `src/styles.css`, `src/routes/__root.tsx`, `src/routes/index.tsx`, `src/routeTree.gen.ts` (committed), `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/icons/`.

---

## Phase 0 — Licensing & housekeeping

### Task 1: Add dual-license files and clean README

**Files:**
- Create: `LICENSE-MIT`
- Create: `LICENSE-APACHE`
- Modify: `README.md` (remove License section, lines 262–265)

- [ ] **Step 1: Create `LICENSE-MIT`**

Use the standard MIT text:

```
MIT License

Copyright (c) 2026 manchhq

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 2: Create `LICENSE-APACHE`**

Fetch the canonical Apache-2.0 text:

Run: `curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE-APACHE`
Expected: file created, ends with the standard appendix. If offline, paste the full Apache License 2.0 text manually.

- [ ] **Step 3: Remove the License section from README**

Delete lines 262–265 (the `---`, `## License`, and the description paragraph) at the end of `README.md`. The new tail of the file should be the `## Status & first milestone` section's final paragraph.

- [ ] **Step 4: Verify**

Run: `tail -n 5 README.md`
Expected: no `## License` heading present.
Run: `ls LICENSE-*`
Expected: `LICENSE-APACHE  LICENSE-MIT`

- [ ] **Step 5: Commit**

```bash
git add LICENSE-MIT LICENSE-APACHE README.md
git commit -m "chore: add dual license files; drop README license section"
```

---

## Phase 1 — Workspace scaffold

### Task 2: pnpm + Turborepo root

**Files:**
- Create: `package.json`, `pnpm-workspace.yaml`, `turbo.json`, `.npmrc`
- Modify: `.gitignore`

- [ ] **Step 1: Create `pnpm-workspace.yaml`**

```yaml
packages:
  - "apps/*"
  - "packages/*"
```

- [ ] **Step 2: Create `.npmrc`**

```
auto-install-peers=true
strict-peer-dependencies=false
```

- [ ] **Step 3: Create root `package.json`**

```json
{
  "name": "manch-monorepo",
  "private": true,
  "packageManager": "pnpm@9.15.0",
  "scripts": {
    "build": "turbo run build",
    "test": "turbo run test",
    "lint": "turbo run lint",
    "storybook": "turbo run storybook",
    "generate": "buf generate",
    "desktop:dev": "pnpm --filter @manch/desktop tauri dev"
  },
  "devDependencies": {
    "turbo": "^2.3.0",
    "@bufbuild/buf": "^1.47.2",
    "typescript": "^5.7.2"
  }
}
```

- [ ] **Step 4: Create `turbo.json`**

```json
{
  "$schema": "https://turbo.build/schema.json",
  "tasks": {
    "build": { "dependsOn": ["^build"], "outputs": ["dist/**"] },
    "test": { "dependsOn": ["^build"] },
    "lint": {},
    "storybook": { "cache": false, "persistent": true },
    "dev": { "cache": false, "persistent": true }
  }
}
```

- [ ] **Step 5: Extend `.gitignore`**

Append:

```
# JS / pnpm
node_modules/
dist/
.turbo/

# Generated protobuf code (regenerated from proto/)
packages/api/src/gen/

# Tauri build output
apps/desktop/src-tauri/target/

# Keep file-based router generated tree (committed on purpose)
!apps/desktop/src/routeTree.gen.ts
```

- [ ] **Step 6: Verify install resolves**

Run: `pnpm install`
Expected: completes with no workspace projects yet (or only root); no error.

- [ ] **Step 7: Commit**

```bash
git add package.json pnpm-workspace.yaml turbo.json .npmrc .gitignore
git commit -m "chore: scaffold pnpm workspace + turborepo root"
```

### Task 3: proto contract + buf config

**Files:**
- Create: `proto/manch/v1/manch.proto`, `buf.yaml`, `buf.gen.yaml`

- [ ] **Step 1: Create `proto/manch/v1/manch.proto`**

```protobuf
syntax = "proto3";

package manch.v1;

// Scaffold service. The real streaming Prompt RPC is deferred to the
// agent-wiring milestone; GetVersion exists to prove the end-to-end path.
service ManchService {
  rpc GetVersion(GetVersionRequest) returns (GetVersionResponse);
}

message GetVersionRequest {}

message GetVersionResponse {
  string version = 1;
}
```

- [ ] **Step 2: Create `buf.yaml`**

```yaml
version: v2
modules:
  - path: proto
lint:
  use:
    - STANDARD
breaking:
  use:
    - FILE
```

- [ ] **Step 3: Create `buf.gen.yaml` (TS only; Rust uses build.rs)**

```yaml
version: v2
inputs:
  - directory: proto
plugins:
  - remote: buf.build/bufbuild/es
    out: packages/api/src/gen
    opt:
      - target=ts
  - remote: buf.build/connectrpc/query-es
    out: packages/api/src/gen
    opt:
      - target=ts
```

- [ ] **Step 4: Verify proto lints**

Run: `pnpm exec buf lint`
Expected: no output (clean) and exit code 0.

- [ ] **Step 5: Commit**

```bash
git add proto buf.yaml buf.gen.yaml
git commit -m "feat: add manch.v1 proto contract + buf config"
```

---

## Phase 2 — Shared UI package (`@manch/ui`)

### Task 4: `@manch/ui` package skeleton + Tailwind/DaisyUI

**Files:**
- Create: `packages/ui/package.json`, `packages/ui/tsconfig.json`, `packages/ui/vite.config.ts`, `packages/ui/src/styles.css`, `packages/ui/src/index.ts`

- [ ] **Step 1: Create `packages/ui/package.json`**

```json
{
  "name": "@manch/ui",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "exports": {
    ".": "./src/index.ts",
    "./styles.css": "./src/styles.css"
  },
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest",
    "lint": "tsc --noEmit",
    "storybook": "storybook dev -p 6006",
    "build-storybook": "storybook build"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tailwindcss/vite": "^4.0.0",
    "tailwindcss": "^4.0.0",
    "daisyui": "^5.0.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.4",
    "vite": "^6.0.0",
    "vitest": "^2.1.8",
    "jsdom": "^25.0.1",
    "@testing-library/react": "^16.1.0",
    "@testing-library/jest-dom": "^6.6.3",
    "@testing-library/user-event": "^14.5.2",
    "storybook": "^8.4.7",
    "@storybook/react-vite": "^8.4.7",
    "@storybook/addon-essentials": "^8.4.7",
    "typescript": "^5.7.2"
  }
}
```

- [ ] **Step 2: Create `packages/ui/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "noEmit": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"]
  },
  "include": ["src"]
}
```

- [ ] **Step 3: Create `packages/ui/src/styles.css`**

```css
@import "tailwindcss";
@plugin "daisyui";
```

- [ ] **Step 4: Create `packages/ui/vite.config.ts`**

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
});
```

- [ ] **Step 5: Create placeholder `packages/ui/src/index.ts`**

```ts
export {};
```

- [ ] **Step 6: Install**

Run: `pnpm install`
Expected: `@manch/ui` deps installed, no errors.

- [ ] **Step 7: Commit**

```bash
git add packages/ui pnpm-lock.yaml
git commit -m "feat(ui): scaffold @manch/ui package with tailwind v4 + daisyui"
```

### Task 5: Button component (TDD) + Vitest setup

**Files:**
- Create: `packages/ui/src/test-setup.ts`, `packages/ui/vitest.config.ts`, `packages/ui/src/Button.test.tsx`, `packages/ui/src/Button.tsx`
- Modify: `packages/ui/src/index.ts`

- [ ] **Step 1: Create `packages/ui/src/test-setup.ts`**

```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 2: Create `packages/ui/vitest.config.ts`**

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
  },
});
```

- [ ] **Step 3: Write the failing test `packages/ui/src/Button.test.tsx`**

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { Button } from "./Button";

describe("Button", () => {
  it("renders its label", () => {
    render(<Button label="Click me" onClick={() => {}} />);
    expect(screen.getByRole("button", { name: "Click me" })).toBeInTheDocument();
  });

  it("calls onClick when pressed", async () => {
    const onClick = vi.fn();
    render(<Button label="Go" onClick={onClick} />);
    await userEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("applies the daisyUI variant class", () => {
    render(<Button label="Primary" variant="primary" onClick={() => {}} />);
    expect(screen.getByRole("button", { name: "Primary" })).toHaveClass("btn-primary");
  });
});
```

- [ ] **Step 4: Run the test, verify it fails**

Run: `pnpm --filter @manch/ui test`
Expected: FAIL — cannot resolve `./Button`.

- [ ] **Step 5: Implement `packages/ui/src/Button.tsx`**

```tsx
export type ButtonVariant = "primary" | "secondary" | "ghost";

export interface ButtonProps {
  label: string;
  onClick: () => void;
  variant?: ButtonVariant;
  disabled?: boolean;
}

const variantClass: Record<ButtonVariant, string> = {
  primary: "btn-primary",
  secondary: "btn-secondary",
  ghost: "btn-ghost",
};

export function Button({ label, onClick, variant = "primary", disabled = false }: ButtonProps) {
  return (
    <button
      type="button"
      className={`btn ${variantClass[variant]}`}
      onClick={onClick}
      disabled={disabled}
    >
      {label}
    </button>
  );
}
```

- [ ] **Step 6: Export from `packages/ui/src/index.ts`**

```ts
export { Button } from "./Button";
export type { ButtonProps, ButtonVariant } from "./Button";
```

- [ ] **Step 7: Run the test, verify it passes**

Run: `pnpm --filter @manch/ui test`
Expected: PASS — 3 tests green.

- [ ] **Step 8: Commit**

```bash
git add packages/ui
git commit -m "feat(ui): add dumb Button component with vitest + RTL tests"
```

### Task 6: Storybook for `@manch/ui`

**Files:**
- Create: `packages/ui/.storybook/main.ts`, `packages/ui/.storybook/preview.ts`, `packages/ui/src/Button.stories.tsx`

- [ ] **Step 1: Create `packages/ui/.storybook/main.ts`**

```ts
import type { StorybookConfig } from "@storybook/react-vite";

const config: StorybookConfig = {
  stories: ["../src/**/*.stories.@(ts|tsx)"],
  addons: ["@storybook/addon-essentials"],
  framework: { name: "@storybook/react-vite", options: {} },
};

export default config;
```

- [ ] **Step 2: Create `packages/ui/.storybook/preview.ts`**

```ts
import type { Preview } from "@storybook/react";
import "../src/styles.css";

const preview: Preview = {
  parameters: {
    controls: { matchers: { color: /(background|color)$/i, date: /Date$/i } },
  },
};

export default preview;
```

- [ ] **Step 3: Create `packages/ui/src/Button.stories.tsx`**

```tsx
import type { Meta, StoryObj } from "@storybook/react";
import { Button } from "./Button";

const meta: Meta<typeof Button> = {
  title: "Components/Button",
  component: Button,
  args: { label: "Click me", onClick: () => {} },
};

export default meta;
type Story = StoryObj<typeof Button>;

export const Primary: Story = { args: { variant: "primary" } };
export const Secondary: Story = { args: { variant: "secondary" } };
export const Ghost: Story = { args: { variant: "ghost" } };
export const Disabled: Story = { args: { disabled: true } };
```

- [ ] **Step 4: Verify Storybook builds (non-interactive check)**

Run: `pnpm --filter @manch/ui build-storybook`
Expected: builds to `storybook-static/` with no errors. (Add `storybook-static/` to `.gitignore` if not covered by `dist/`.)

- [ ] **Step 5: Commit**

```bash
git add packages/ui .gitignore
git commit -m "feat(ui): add storybook + Button stories"
```

---

## Phase 3 — Server + API client

### Task 7: `crates/manch-server` (TDD on the stub service)

**Files:**
- Create: `crates/manch-server/Cargo.toml`, `crates/manch-server/build.rs`, `crates/manch-server/src/lib.rs`, `crates/manch-server/src/service.rs`, `crates/manch-server/src/main.rs`
- Modify: root `Cargo.toml` (workspace members + deps)

- [ ] **Step 1: Add the crate to the workspace in `Cargo.toml`**

Change `members` to:

```toml
members = ["crates/manch-protocol", "crates/manch-server"]
```

And add to `[workspace.dependencies]`:

```toml
# Server stack
connectrpc = { version = "0.7", features = ["axum", "client"] }
connectrpc-build = "0.7"
buffa = { version = "0.7", features = ["json"] }
buffa-types = { version = "0.7", features = ["json"] }
axum = "0.8"
http-body = "1"
```

(Note: `tokio` is already present; extend its features to include `rt-multi-thread` and `net`: `tokio = { version = "1", features = ["sync", "rt", "rt-multi-thread", "net", "macros"] }`.)

- [ ] **Step 2: Create `crates/manch-server/Cargo.toml`**

```toml
[package]
name = "manch-server"
description = "Optional, domain-free self-hostable Manch server exposing the core over ConnectRPC."
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
manch-protocol = { workspace = true }
connectrpc = { workspace = true }
buffa = { workspace = true }
buffa-types = { workspace = true }
axum = { workspace = true }
http-body = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }

[build-dependencies]
connectrpc-build = { workspace = true }

[[bin]]
name = "manch-server"
path = "src/main.rs"
```

- [ ] **Step 3: Create `crates/manch-server/build.rs`**

```rust
fn main() {
    connectrpc_build::Config::new()
        .files(&["../../proto/manch/v1/manch.proto"])
        .includes(&["../../proto"])
        .compile()
        .expect("failed to compile proto");
}
```

- [ ] **Step 4: Create `crates/manch-server/src/lib.rs`**

```rust
//! Manch ConnectRPC server. Embeds the Manch core (a stub for now) and exposes
//! it over the Connect / gRPC-web protocol.

pub mod proto {
    connectrpc::include_generated!();
}

pub mod service;

pub use service::ManchServiceImpl;
```

- [ ] **Step 5: Write the failing test + stub in `crates/manch-server/src/service.rs`**

```rust
use connectrpc::{RequestContext, Response, ServiceRequest, ServiceResult};

use crate::proto::manch::v1::{
    GetVersionRequest, GetVersionResponse, ManchService,
};

/// Stub implementation. Returns the crate version; real RPCs arrive with the
/// agent-wiring milestone.
#[derive(Clone, Default)]
pub struct ManchServiceImpl;

impl ManchService for ManchServiceImpl {
    async fn get_version(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, GetVersionRequest>,
    ) -> ServiceResult<GetVersionResponse> {
        Response::ok(GetVersionResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_version_returns_crate_version() {
        let svc = ManchServiceImpl;
        let req = ServiceRequest::from_message(GetVersionRequest::default());
        let resp = svc
            .get_version(RequestContext::default(), req)
            .await
            .expect("ok");
        assert_eq!(resp.into_message().version, env!("CARGO_PKG_VERSION"));
    }
}
```

> If `ServiceRequest::from_message` / `RequestContext::default` / `Response::into_message` differ in connectrpc 0.7, consult the generated code under `target/.../out` and the crate docs, and adapt the test to the actual constructors. The behavioral assertion (version string round-trips) stays the same.

- [ ] **Step 6: Run the test, verify it fails to compile/pass**

Run: `cargo test -p manch-server`
Expected: FAIL (service not yet wired / generated code missing on first run, then assertion-driven once it compiles).

- [ ] **Step 7: Create `crates/manch-server/src/main.rs`**

```rust
use std::sync::Arc;

use axum::{routing::get, Router as AxumRouter};
use connectrpc::Router as ConnectRouter;
use manch_server::ManchServiceImpl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = Arc::new(ManchServiceImpl);
    let connect = service.register(ConnectRouter::new());

    let app = AxumRouter::new()
        .route("/health", get(|| async { "OK" }))
        .fallback_service(connect.into_axum_service());

    let addr = "127.0.0.1:8787";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("manch-server listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 8: Run the test, verify it passes**

Run: `cargo test -p manch-server`
Expected: PASS.

- [ ] **Step 9: Verify the server boots**

Run: `cargo run -p manch-server` (then Ctrl-C)
Expected: prints `manch-server listening on http://127.0.0.1:8787`.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock crates/manch-server
git commit -m "feat(server): connectrpc + axum server with GetVersion stub"
```

### Task 8: `@manch/api` generated client + transport

**Files:**
- Create: `packages/api/package.json`, `packages/api/tsconfig.json`, `packages/api/src/transport.ts`, `packages/api/src/index.ts`
- Generated (gitignored): `packages/api/src/gen/**`

- [ ] **Step 1: Create `packages/api/package.json`**

```json
{
  "name": "@manch/api",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "exports": { ".": "./src/index.ts" },
  "scripts": {
    "generate": "buf generate",
    "lint": "tsc --noEmit"
  },
  "dependencies": {
    "@connectrpc/connect": "^2.0.0",
    "@connectrpc/connect-web": "^2.0.0",
    "@connectrpc/connect-query": "^2.0.0",
    "@bufbuild/protobuf": "^2.2.3",
    "@tanstack/react-query": "^5.62.0",
    "remeda": "^2.17.0",
    "valibot": "^1.0.0"
  },
  "devDependencies": {
    "@bufbuild/buf": "^1.47.2",
    "@bufbuild/protoc-gen-es": "^2.2.3",
    "@connectrpc/protoc-gen-connect-query": "^2.0.0",
    "typescript": "^5.7.2"
  }
}
```

- [ ] **Step 2: Create `packages/api/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "noEmit": true
  },
  "include": ["src"]
}
```

- [ ] **Step 3: Point `buf.gen.yaml` at local plugins**

Confirm root `buf.gen.yaml` uses the locally installed plugins so generation is reproducible:

```yaml
version: v2
inputs:
  - directory: proto
plugins:
  - local: ./node_modules/.bin/protoc-gen-es
    out: packages/api/src/gen
    opt:
      - target=ts
  - local: ./node_modules/.bin/protoc-gen-connect-query
    out: packages/api/src/gen
    opt:
      - target=ts
```

- [ ] **Step 4: Install and generate**

Run: `pnpm install`
Run: `pnpm exec buf generate`
Expected: files appear under `packages/api/src/gen/manch/v1/` (e.g. `manch_pb.ts`, `manch-ManchService_connectquery.ts`). These are gitignored.

- [ ] **Step 5: Create `packages/api/src/transport.ts`**

```ts
import { createConnectTransport } from "@connectrpc/connect-web";

/** Default localhost endpoint of manch-server. */
export const DEFAULT_BASE_URL = "http://127.0.0.1:8787";

export function createManchTransport(baseUrl: string = DEFAULT_BASE_URL) {
  return createConnectTransport({ baseUrl });
}
```

- [ ] **Step 6: Create `packages/api/src/index.ts`**

```ts
export * from "./transport";
export { ManchService } from "./gen/manch/v1/manch_pb";
export * as versionQuery from "./gen/manch/v1/manch-ManchService_connectquery";
```

> Generated filenames depend on the protoc-gen-es / connect-query version. After Step 4, list `packages/api/src/gen/manch/v1/` and adjust the two re-export paths to the actual emitted filenames.

- [ ] **Step 7: Typecheck**

Run: `pnpm --filter @manch/api lint`
Expected: no type errors.

- [ ] **Step 8: Commit**

```bash
git add packages/api buf.gen.yaml pnpm-lock.yaml
git commit -m "feat(api): generated connect client + connect-query hooks + transport"
```

---

## Phase 4 — Tauri desktop app

### Task 9: Scaffold the Tauri 2 app shell

**Files:**
- Create: `apps/desktop/package.json`, `apps/desktop/tsconfig.json`, `apps/desktop/vite.config.ts`, `apps/desktop/index.html`, `apps/desktop/src/styles.css`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/build.rs`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src-tauri/src/lib.rs`
- Modify: root `Cargo.toml` (add `apps/desktop/src-tauri` member)

- [ ] **Step 1: Add the Tauri crate to the Cargo workspace**

In root `Cargo.toml`:

```toml
members = ["crates/manch-protocol", "crates/manch-server", "apps/desktop/src-tauri"]
```

- [ ] **Step 2: Create `apps/desktop/package.json`**

```json
{
  "name": "@manch/desktop",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "lint": "tsc --noEmit",
    "tauri": "tauri"
  },
  "dependencies": {
    "@manch/ui": "workspace:*",
    "@manch/api": "workspace:*",
    "@tanstack/react-router": "^1.87.0",
    "@tanstack/react-query": "^5.62.0",
    "@connectrpc/connect-query": "^2.0.0",
    "@uidotdev/usehooks": "^2.4.1",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.1.0",
    "@tanstack/router-plugin": "^1.87.0",
    "@vitejs/plugin-react": "^4.3.4",
    "@tailwindcss/vite": "^4.0.0",
    "tailwindcss": "^4.0.0",
    "daisyui": "^5.0.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "vite": "^6.0.0",
    "typescript": "^5.7.2"
  }
}
```

> `react-hookz` was named in the spec; `@uidotdev/usehooks` is a drop-in equivalent if `react-hookz`'s peer ranges fight React 19. Prefer `@react-hookz/web` if its React 19 support has landed at implementation time — pick one and stay consistent.

- [ ] **Step 3: Create `apps/desktop/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "noEmit": true
  },
  "include": ["src"]
}
```

- [ ] **Step 4: Create `apps/desktop/vite.config.ts`**

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { TanStackRouterVite } from "@tanstack/router-plugin/vite";

export default defineConfig({
  plugins: [
    TanStackRouterVite({ target: "react", autoCodeSplitting: true }),
    react(),
    tailwindcss(),
  ],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
```

- [ ] **Step 5: Create `apps/desktop/index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Manch</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Create `apps/desktop/src/styles.css`**

```css
@import "tailwindcss";
@plugin "daisyui";
```

- [ ] **Step 7: Create `apps/desktop/src-tauri/Cargo.toml`**

```toml
[package]
name = "manch-desktop"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[lib]
name = "manch_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 8: Create `apps/desktop/src-tauri/build.rs`**

```rust
fn main() {
    tauri_build::build();
}
```

- [ ] **Step 9: Create `apps/desktop/src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Manch",
  "version": "0.0.0",
  "identifier": "hq.manch.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "pnpm --filter @manch/desktop dev",
    "beforeBuildCommand": "pnpm --filter @manch/desktop build"
  },
  "app": {
    "windows": [{ "title": "Manch", "width": 1000, "height": 700 }],
    "security": { "csp": null }
  },
  "bundle": { "active": true, "targets": "all", "icon": ["icons/icon.png"] }
}
```

- [ ] **Step 10: Create `apps/desktop/src-tauri/src/lib.rs`**

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 11: Create `apps/desktop/src-tauri/src/main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    manch_desktop_lib::run();
}
```

- [ ] **Step 12: Add an app icon**

Run: `pnpm --filter @manch/desktop exec tauri icon` (uses a default source) OR drop a 512×512 PNG at `apps/desktop/src-tauri/icons/icon.png` and let `tauri icon` generate the rest.
Expected: `apps/desktop/src-tauri/icons/` populated.

- [ ] **Step 13: Install**

Run: `pnpm install`
Expected: workspace links `@manch/ui` and `@manch/api`; no errors.

- [ ] **Step 14: Commit**

```bash
git add apps/desktop Cargo.toml Cargo.lock pnpm-lock.yaml
git commit -m "feat(desktop): scaffold tauri 2 app shell"
```

### Task 10: Wire routing, query, and the version display

**Files:**
- Create: `apps/desktop/src/main.tsx`, `apps/desktop/src/routes/__root.tsx`, `apps/desktop/src/routes/index.tsx`, `apps/desktop/src/routeTree.gen.ts` (auto-generated, committed)
- Create: `packages/ui/src/VersionBadge.tsx`, `packages/ui/src/VersionBadge.test.tsx`, `packages/ui/src/VersionBadge.stories.tsx`; modify `packages/ui/src/index.ts`

- [ ] **Step 1: Write the failing test for a dumb VersionBadge `packages/ui/src/VersionBadge.test.tsx`**

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { VersionBadge } from "./VersionBadge";

describe("VersionBadge", () => {
  it("shows the version when provided", () => {
    render(<VersionBadge version="1.2.3" />);
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
  });

  it("shows a loading state when version is undefined", () => {
    render(<VersionBadge version={undefined} loading />);
    expect(screen.getByText("loading…")).toBeInTheDocument();
  });

  it("shows an error message when error is set", () => {
    render(<VersionBadge version={undefined} error="boom" />);
    expect(screen.getByText("boom")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `pnpm --filter @manch/ui test`
Expected: FAIL — cannot resolve `./VersionBadge`.

- [ ] **Step 3: Implement `packages/ui/src/VersionBadge.tsx`**

```tsx
export interface VersionBadgeProps {
  version: string | undefined;
  loading?: boolean;
  error?: string;
}

export function VersionBadge({ version, loading = false, error }: VersionBadgeProps) {
  if (error) return <span className="badge badge-error">{error}</span>;
  if (loading || version === undefined) return <span className="badge">loading…</span>;
  return <span className="badge badge-success">v{version}</span>;
}
```

- [ ] **Step 4: Export it from `packages/ui/src/index.ts`**

```ts
export { Button } from "./Button";
export type { ButtonProps, ButtonVariant } from "./Button";
export { VersionBadge } from "./VersionBadge";
export type { VersionBadgeProps } from "./VersionBadge";
```

- [ ] **Step 5: Run the test, verify it passes**

Run: `pnpm --filter @manch/ui test`
Expected: PASS — all tests green.

- [ ] **Step 6: Add `packages/ui/src/VersionBadge.stories.tsx`**

```tsx
import type { Meta, StoryObj } from "@storybook/react";
import { VersionBadge } from "./VersionBadge";

const meta: Meta<typeof VersionBadge> = {
  title: "Components/VersionBadge",
  component: VersionBadge,
};

export default meta;
type Story = StoryObj<typeof VersionBadge>;

export const Loaded: Story = { args: { version: "0.0.0" } };
export const Loading: Story = { args: { version: undefined, loading: true } };
export const Errored: Story = { args: { version: undefined, error: "unreachable" } };
```

- [ ] **Step 7: Create `apps/desktop/src/routes/__root.tsx`**

```tsx
import { createRootRoute, Outlet } from "@tanstack/react-router";
import "../styles.css";

export const Route = createRootRoute({
  component: () => (
    <div className="min-h-screen bg-base-200 p-8">
      <Outlet />
    </div>
  ),
});
```

- [ ] **Step 8: Create `apps/desktop/src/routes/index.tsx` (smart parent → dumb UI)**

```tsx
import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { createClient } from "@connectrpc/connect";
import { ManchService, createManchTransport } from "@manch/api";
import { VersionBadge } from "@manch/ui";

export const Route = createFileRoute("/")({
  component: Home,
});

const client = createClient(ManchService, createManchTransport());

function Home() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["version"],
    queryFn: () => client.getVersion({}),
  });

  return (
    <main className="flex flex-col gap-4">
      <h1 className="text-2xl font-bold">Manch</h1>
      <div className="flex items-center gap-2">
        <span>server version:</span>
        <VersionBadge
          version={data?.version}
          loading={isLoading}
          error={error ? "unreachable" : undefined}
        />
      </div>
    </main>
  );
}
```

> If `@manch/api` exposes the typed `versionQuery` connect-query hook cleanly, prefer `useQuery(versionQuery.getVersion)` from `@connectrpc/connect-query` over a hand-rolled `queryFn`. Use whichever the generated output supports; the rendered result is identical.

- [ ] **Step 9: Create `apps/desktop/src/main.tsx`**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { routeTree } from "./routeTree.gen";

const router = createRouter({ routeTree });
const queryClient = new QueryClient();

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 10: Generate the route tree (committed)**

Run: `pnpm --filter @manch/desktop dev` briefly (the TanStackRouterVite plugin writes `src/routeTree.gen.ts`), then stop it. Confirm the file exists.
Expected: `apps/desktop/src/routeTree.gen.ts` created. It is committed (the `.gitignore` exception keeps it tracked).

- [ ] **Step 11: Typecheck the app + run UI tests**

Run: `pnpm --filter @manch/ui test`
Expected: PASS.
Run: `pnpm --filter @manch/desktop lint`
Expected: no type errors.

- [ ] **Step 12: End-to-end manual check**

Run in one terminal: `cargo run -p manch-server`
Run in another: `pnpm --filter @manch/desktop tauri dev`
Expected: the Manch window opens and shows `server version: v0.0.0` (green badge). Stopping the server and reloading shows the error badge.

- [ ] **Step 13: Commit**

```bash
git add apps/desktop packages/ui
git commit -m "feat(desktop): wire router + query + version display via @manch/ui"
```

---

## Phase 5 — Aggregate verification

### Task 11: Turbo pipeline + final checks

**Files:** none new (verification only)

- [ ] **Step 1: Run the full JS/TS test pipeline**

Run: `pnpm turbo run test`
Expected: `@manch/ui` tests PASS; other packages report no tests (ok).

- [ ] **Step 2: Run lint across the workspace**

Run: `pnpm turbo run lint`
Expected: all packages typecheck clean.

- [ ] **Step 3: Build the Rust workspace + test**

Run: `cargo build && cargo test`
Expected: all crates compile; `manch-server` test passes.

- [ ] **Step 4: Confirm gitignore correctness**

Run: `git status --porcelain`
Expected: `packages/api/src/gen/` is NOT listed (ignored); `apps/desktop/src/routeTree.gen.ts` IS tracked.

- [ ] **Step 5: Final commit (if anything pending)**

```bash
git add -A
git commit -m "chore: aggregate verification for monorepo scaffold"
```

---

## Notes for the implementer

- **Version drift:** the pinned npm/crate versions are accurate as of 2026-06; if `pnpm install` or `cargo build` reports an incompatibility, bump to the nearest working version and keep the API usage shown here.
- **connectrpc 0.7 API surface is pre-1.0.** If constructor/method names in Task 7's test differ, inspect the generated code and the crate docs, then adapt the *mechanics* — not the asserted behavior.
- **Generated filenames** (Task 8) depend on plugin versions; list the `gen/` dir and fix re-export paths after first generation.
- **Dumb-component rule:** nothing in `packages/ui` may import from `@manch/api`, `@tanstack/*`, or do data fetching. Keep that boundary; it is the whole point of the split.
