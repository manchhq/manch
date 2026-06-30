import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createStore, Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => () => {} }));
import SchedulePage from "./SchedulePage";
import { activeWorkspaceIdAtom } from "../store/atoms";

function wrap(ui: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const store = createStore();
  store.set(activeWorkspaceIdAtom, "w1");
  return <QueryClientProvider client={qc}><JotaiProvider store={store}>{ui}</JotaiProvider></QueryClientProvider>;
}

describe("SchedulePage", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });

  it("lists schedules and creates one", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_schedules" ? Promise.resolve([{ id: "s1", workspace_id: "w1", target: "Discovery team", cadence: "daily", next_run: "2026-07-01T09:00:00Z" }]) : Promise.resolve([]));
    render(wrap(<SchedulePage />));
    await waitFor(() => expect(screen.getByText("Discovery team")).toBeTruthy());
  });
});
