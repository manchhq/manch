import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createStore, Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => vi.fn() }));

// Mock the engine so single-provider path is instant (avoids 300 ms sleep delays)
vi.mock("../engine/mockEngine", () => ({
  mockEngine: {
    async *send() {
      yield { kind: "token", text: "hi" };
      yield { kind: "done" };
    },
  },
}));

import Stage from "./Stage";
import {
  conversationsAtom,
  activeIdAtom,
  compareProvidersAtom,
} from "../store/atoms";

function makeStore() {
  const store = createStore();
  store.set(conversationsAtom, [{ id: "c1", title: "T", messages: [], toolCalls: [] }]);
  store.set(activeIdAtom, "c1");
  return store;
}

function wrap(ui: React.ReactNode, store: ReturnType<typeof createStore>) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return (
    <QueryClientProvider client={qc}>
      <JotaiProvider store={store}>{ui}</JotaiProvider>
    </QueryClientProvider>
  );
}

describe("Stage", () => {
  beforeEach(() => {
    invoke.mockReset();
    localStorage.clear();
  });

  it("shows CompareView when >1 providers are selected and a prompt is sent", async () => {
    const store = makeStore();
    store.set(compareProvidersAtom, ["anthropic", "claude-code"]);

    invoke.mockImplementation((cmd: string) =>
      cmd === "cross_verify"
        ? Promise.resolve({
            reports: [
              { provider: "anthropic", text: "A" },
              { provider: "claude-code", text: "B" },
            ],
            summary: "agree",
          })
        : cmd === "list_configured_providers"
          ? Promise.resolve(["anthropic"])
          : Promise.resolve([]),
    );

    render(wrap(<Stage />, store));

    const inputEl = screen.getByPlaceholderText("Message…");
    await userEvent.type(inputEl, "Hello");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));

    await waitFor(() =>
      expect(screen.getByText(/cross-verification/i)).toBeTruthy(),
    );
    expect(invoke).toHaveBeenCalledWith("cross_verify", expect.objectContaining({ providers: ["anthropic", "claude-code"] }));
    expect(screen.getByText("anthropic")).toBeTruthy();
    expect(screen.getByText("claude-code")).toBeTruthy();
  });

  it("uses single-provider path when compareProvidersAtom is empty (no-regression guard)", async () => {
    const store = makeStore();
    // compareProvidersAtom is empty — compare mode NOT active

    invoke.mockImplementation((cmd: string) =>
      cmd === "list_configured_providers" ? Promise.resolve(["anthropic"]) : Promise.resolve([]),
    );

    render(wrap(<Stage />, store));

    // CompareView must not be present in the initial render
    expect(screen.queryByText(/cross-verification/i)).toBeNull();

    // Submitting a prompt must NOT call the cross_verify command
    const inputEl = screen.getByPlaceholderText("Message…");
    await userEvent.type(inputEl, "Hello");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));

    // Give React Query and the mock engine a tick to settle
    await waitFor(() =>
      expect(invoke).not.toHaveBeenCalledWith("cross_verify", expect.anything()),
    );

    // CompareView still absent; normal transcript area is rendered
    expect(screen.queryByText(/cross-verification/i)).toBeNull();
  });

  it("disables send and nudges to Settings when no provider is configured", async () => {
    const store = makeStore();
    // list_configured_providers → empty list (no AI configured)
    invoke.mockResolvedValue([]);

    render(wrap(<Stage />, store));

    // Wait for the query to settle
    await waitFor(() =>
      expect(screen.getByText(/no ai provider/i)).toBeTruthy(),
    );

    // Send button must be disabled
    const sendBtn = screen.getByRole("button", { name: /send/i });
    expect((sendBtn as HTMLButtonElement).disabled).toBe(true);

    // A link/button to settings must be present
    const settingsLink = screen.getByRole("button", { name: /configure/i });
    expect(settingsLink).toBeTruthy();
  });

  it("shows an inviting empty state when there are no conversations and clicking 'New conversation' creates one", async () => {
    // Empty store — no conversations, no active id
    const store = createStore();

    invoke.mockImplementation((cmd: string) =>
      cmd === "list_configured_providers" ? Promise.resolve(["anthropic"]) : Promise.resolve([]),
    );

    render(wrap(<Stage />, store));

    // Empty state heading must be visible
    expect(screen.getByText("No conversation yet")).toBeTruthy();

    // Action button must be present
    const newBtn = screen.getByRole("button", { name: /new conversation/i });
    expect(newBtn).toBeTruthy();

    // Clicking the button should create a conversation and switch to the main view
    await userEvent.click(newBtn);

    // After creation the empty state disappears and the composer is shown
    await waitFor(() =>
      expect(screen.queryByText("No conversation yet")).toBeNull(),
    );
    expect(screen.getByPlaceholderText("Message…")).toBeTruthy();
  });
});
