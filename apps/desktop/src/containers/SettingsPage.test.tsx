import { render, screen, fireEvent, waitFor } from "@testing-library/react";
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
    // Each provider label renders twice: the providers list <span> and the
    // provider <select> <option>.
    const anthropicItems = await screen.findAllByText(/Anthropic/i);
    expect(anthropicItems.length).toBeGreaterThanOrEqual(2);
    const claudeCodeItems = await screen.findAllByText(/Claude Code/i);
    expect(claudeCodeItems.length).toBeGreaterThanOrEqual(2);
    const geminiItems = await screen.findAllByText(/Google Gemini/i);
    expect(geminiItems.length).toBeGreaterThanOrEqual(2);
    const openaiItems = await screen.findAllByText(/OpenAI/i);
    expect(openaiItems.length).toBeGreaterThanOrEqual(2);
    const geminiCliItems = await screen.findAllByText(/Gemini CLI/i);
    expect(geminiCliItems.length).toBeGreaterThanOrEqual(2);
    const codexItems = await screen.findAllByText(/Codex CLI/i);
    expect(codexItems.length).toBeGreaterThanOrEqual(2);
  });

  it("fetches and renders a model dropdown for a configured BYOK provider, and persists a change", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_configured_providers") return Promise.resolve(["anthropic"]);
      if (cmd === "list_models") {
        return Promise.resolve([
          { id: "claude-opus-4-8", display_name: "Claude Opus 4.8" },
          { id: "claude-sonnet-5", display_name: "Claude Sonnet 5" },
        ]);
      }
      return Promise.resolve([]);
    });
    render(wrap(<SettingsPage />));
    const select = await screen.findByLabelText(/anthropic model/i);
    expect(invoke).toHaveBeenCalledWith("list_models", { provider: "anthropic" });
    fireEvent.change(select, { target: { value: "claude-sonnet-5" } });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_model", { provider: "anthropic", model: "claude-sonnet-5" }),
    );
  });

  it("defaults the dropdown to the persisted model from get_model", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_configured_providers") return Promise.resolve(["anthropic"]);
      if (cmd === "list_models") {
        return Promise.resolve([
          { id: "claude-opus-4-8", display_name: "Claude Opus 4.8" },
          { id: "claude-sonnet-5", display_name: "Claude Sonnet 5" },
        ]);
      }
      if (cmd === "get_model") return Promise.resolve("claude-sonnet-5");
      return Promise.resolve([]);
    });
    render(wrap(<SettingsPage />));
    const select = (await screen.findByLabelText(/anthropic model/i)) as HTMLSelectElement;
    expect(invoke).toHaveBeenCalledWith("get_model", { provider: "anthropic" });
    await waitFor(() => expect(select.value).toBe("claude-sonnet-5"));
  });

  it("fetches and renders a model dropdown for a configured gemini BYOK provider", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_configured_providers") return Promise.resolve(["gemini"]);
      if (cmd === "list_models") {
        return Promise.resolve([{ id: "gemini-2.5-pro", display_name: "Gemini 2.5 Pro" }]);
      }
      return Promise.resolve([]);
    });
    render(wrap(<SettingsPage />));
    const select = await screen.findByLabelText(/gemini model/i);
    expect(invoke).toHaveBeenCalledWith("list_models", { provider: "gemini" });
    expect(select).toBeTruthy();
  });

  it("does not render a model dropdown for a CLI provider even when configured", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_configured_providers" ? Promise.resolve(["claude-code"]) : Promise.resolve([]));
    render(wrap(<SettingsPage />));
    await screen.findAllByText(/Claude Code/i);
    expect(screen.queryByLabelText(/claude-code model/i)).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("list_models", expect.anything());
  });

  it("does not render a model dropdown for the codex CLI provider even when configured", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_configured_providers" ? Promise.resolve(["codex"]) : Promise.resolve([]));
    render(wrap(<SettingsPage />));
    await screen.findAllByText(/Codex CLI/i);
    expect(screen.queryByLabelText(/codex model/i)).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("list_models", expect.anything());
  });
});
