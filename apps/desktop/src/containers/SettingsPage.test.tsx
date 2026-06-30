import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Provider as JotaiProvider } from "jotai";
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import SettingsPage from "./SettingsPage";

const wrap = (ui: React.ReactNode) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <JotaiProvider>{ui}</JotaiProvider>
    </QueryClientProvider>
  );
};

describe("SettingsPage", () => {
  beforeEach(() => {
    invoke.mockReset();
    localStorage.clear();
  });

  it("switches theme via the picker", async () => {
    invoke.mockResolvedValue([]); // providers + workspaces queries
    render(wrap(<SettingsPage />));
    const dracula = await screen.findByRole("radio", { name: "dracula" });
    fireEvent.click(dracula);
    expect(localStorage.getItem("manch.theme")).toContain("dracula");
  });

  it("renders all providers from ALL_PROVIDERS", async () => {
    invoke.mockResolvedValue([]);
    render(wrap(<SettingsPage />));
    const anthropicItems = await screen.findAllByText(/Anthropic/i);
    expect(anthropicItems.length).toBeGreaterThan(0);
    const claudeCodeItems = await screen.findAllByText(/Claude Code/i);
    expect(claudeCodeItems.length).toBeGreaterThan(0);
  });
});
