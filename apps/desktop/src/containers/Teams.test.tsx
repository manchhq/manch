import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createStore, Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => () => {} }));
import Teams from "./Teams";
import { activeWorkspaceIdAtom } from "../store/atoms";

function wrap(ui: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const store = createStore();
  store.set(activeWorkspaceIdAtom, "w1");
  return <QueryClientProvider client={qc}><JotaiProvider store={store}>{ui}</JotaiProvider></QueryClientProvider>;
}

describe("Teams", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });
  it("lists teams for the active workspace", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_teams" ? Promise.resolve([{ id: "tm_1", workspace_id: "w1", name: "Discovery", problem: "p", members: [{ role: "R", provider: "anthropic" }], capabilities: [] }]) : Promise.resolve([]));
    render(wrap(<Teams />));
    await waitFor(() => expect(screen.getByText("Discovery")).toBeTruthy());
  });
});
