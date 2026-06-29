import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Provider, createStore } from "jotai";
import type { ReactNode } from "react";
import { conversationsAtom, newConversation } from "../store/atoms";

vi.mock("../lib/api", () => ({
  saveApiKey: vi.fn(), listConfiguredProviders: vi.fn().mockResolvedValue([]), sendPrompt: vi.fn(),
}));

// Import the component under test indirectly: render the Home component.
// Export Home for testability:
import { Home } from "./index";

function wrap(children: ReactNode, store = createStore()) {
  return (
    <QueryClientProvider client={new QueryClient()}>
      <Provider store={store}>{children}</Provider>
    </QueryClientProvider>
  );
}

describe("Home route", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("shows Settings/first-run when there are no conversations", () => {
    render(wrap(<Home />));
    expect(screen.getByText("Add a provider key")).toBeTruthy();
  });

  it("shows the 3-pane stage when a conversation exists", () => {
    const store = createStore();
    store.set(conversationsAtom, [newConversation("t")]);
    render(wrap(<Home />, store));
    expect(screen.getByText("Green Room")).toBeTruthy();
    expect(screen.getByText("Performance")).toBeTruthy();
  });

  it("clicking 'New conversation' from empty store transitions to the 3-pane stage", async () => {
    const user = userEvent.setup();
    render(wrap(<Home />));
    const btn = screen.getByRole("button", { name: "New conversation" });
    await user.click(btn);
    expect(screen.getByText("Green Room")).toBeTruthy();
    expect(screen.getByText("Performance")).toBeTruthy();
  });
});
