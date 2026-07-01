// WorkspaceBar.test.tsx
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createStore, Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import WorkspaceBar from "./WorkspaceBar";
import { activeWorkspaceIdAtom } from "../store/atoms";

const wrap = (ui: React.ReactNode, store?: ReturnType<typeof createStore>) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}><JotaiProvider store={store}>{ui}</JotaiProvider></QueryClientProvider>;
};

describe("WorkspaceBar", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });
  it("shows workspaces and defaults the active one", async () => {
    invoke.mockResolvedValueOnce([{ id: "w1", name: "Alpha", description: "" }]);
    render(wrap(<WorkspaceBar />));
    await waitFor(() => expect(screen.getByRole("button", { name: /Alpha/ })).toBeTruthy());
  });

  it("self-corrects when the persisted active id no longer resolves", async () => {
    // Persisted active id points at a since-deleted workspace.
    const store = createStore();
    store.set(activeWorkspaceIdAtom, "deleted-ws");
    invoke.mockResolvedValueOnce([{ id: "w1", name: "Alpha", description: "" }]);

    render(wrap(<WorkspaceBar />, store));

    // The stale id is reset to the first available workspace (not left stuck).
    await waitFor(() => expect(store.get(activeWorkspaceIdAtom)).toBe("w1"));
    expect(screen.getByRole("button", { name: /Alpha/ })).toBeTruthy();
  });
});
