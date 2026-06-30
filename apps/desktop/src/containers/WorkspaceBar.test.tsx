// WorkspaceBar.test.tsx
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import WorkspaceBar from "./WorkspaceBar";

const wrap = (ui: React.ReactNode) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}><JotaiProvider>{ui}</JotaiProvider></QueryClientProvider>;
};

describe("WorkspaceBar", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });
  it("shows workspaces and defaults the active one", async () => {
    invoke.mockResolvedValueOnce([{ id: "w1", name: "Alpha", description: "" }]);
    render(wrap(<WorkspaceBar />));
    await waitFor(() => expect(screen.getByRole("button", { name: /Alpha/ })).toBeTruthy());
  });
});
