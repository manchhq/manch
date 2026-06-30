import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createStore, Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => () => {} }));
import SearchPage from "./SearchPage";
import { activeWorkspaceIdAtom } from "../store/atoms";

function wrap(ui: React.ReactNode, workspaceId: string | null = "w1") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const store = createStore();
  if (workspaceId) store.set(activeWorkspaceIdAtom, workspaceId);
  return <QueryClientProvider client={qc}><JotaiProvider store={store}>{ui}</JotaiProvider></QueryClientProvider>;
}

describe("SearchPage", () => {
  beforeEach(() => { invoke.mockReset(); localStorage.clear(); });

  it("shows EmptyState when no workspace is active", () => {
    render(wrap(<SearchPage />, null));
    expect(screen.getByText("No workspace")).toBeTruthy();
  });

  it("renders a search hit after submitting a query", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "search"
        ? Promise.resolve([{ kind: "team", id: "tm_1", title: "Discovery", snippet: "best team" }])
        : Promise.resolve([]),
    );
    render(wrap(<SearchPage />));
    await userEvent.type(screen.getByRole("searchbox"), "Disco");
    await userEvent.click(screen.getByRole("button", { name: /search/i }));
    await waitFor(() => expect(screen.getByText("Discovery")).toBeTruthy());
  });

  it("shows no-results EmptyState when query returns empty", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "search" ? Promise.resolve([]) : Promise.resolve([]),
    );
    render(wrap(<SearchPage />));
    await userEvent.type(screen.getByRole("searchbox"), "xyz");
    await userEvent.click(screen.getByRole("button", { name: /search/i }));
    await waitFor(() => expect(screen.getByText("No results")).toBeTruthy());
  });
});
