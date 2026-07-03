// ProviderSettings.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ProviderSettings } from "./ProviderSettings";

const all = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];

describe("ProviderSettings", () => {
  it("lists configured providers", () => {
    render(<ProviderSettings all={all} configured={["anthropic"]} onSave={() => {}} />);
    // "Anthropic" appears twice: the list <span> and the select <option>.
    expect(screen.getAllByText(/Anthropic/).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText(/configured/i)).toBeTruthy();
  });

  it("submits a provider + key", async () => {
    const onSave = vi.fn();
    render(<ProviderSettings all={all} configured={[]} onSave={onSave} />);
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "sk-test" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    await screen.findByRole("button", { name: /save key/i });
    expect(onSave).toHaveBeenCalledWith("anthropic", "sk-test");
  });

  it("renders a model dropdown for a configured BYOK provider and reports changes", () => {
    const onModelChange = vi.fn();
    const models = {
      anthropic: [
        { id: "claude-opus-4-8", displayName: "Claude Opus 4.8" },
        { id: "claude-sonnet-5", displayName: "Claude Sonnet 5" },
      ],
    };
    render(
      <ProviderSettings
        all={all}
        configured={["anthropic"]}
        onSave={() => {}}
        models={models}
        onModelChange={onModelChange}
      />,
    );
    const select = screen.getByLabelText(/anthropic model/i);
    expect(screen.getByRole("option", { name: "Claude Opus 4.8" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Claude Sonnet 5" })).toBeTruthy();
    fireEvent.change(select, { target: { value: "claude-sonnet-5" } });
    expect(onModelChange).toHaveBeenCalledWith("anthropic", "claude-sonnet-5");
  });

  it("defaults the dropdown to the persisted model, not the first listed", () => {
    render(
      <ProviderSettings
        all={all}
        configured={["anthropic"]}
        onSave={() => {}}
        models={{
          anthropic: [
            { id: "claude-opus-4-8", displayName: "Claude Opus 4.8" },
            { id: "claude-sonnet-5", displayName: "Claude Sonnet 5" },
          ],
        }}
        selectedModels={{ anthropic: "claude-sonnet-5" }}
        onModelChange={() => {}}
      />,
    );
    const select = screen.getByLabelText(/anthropic model/i) as HTMLSelectElement;
    expect(select.value).toBe("claude-sonnet-5");
  });

  it("falls back to a single-option select when only one model is available", () => {
    render(
      <ProviderSettings
        all={all}
        configured={["anthropic"]}
        onSave={() => {}}
        models={{ anthropic: [{ id: "only-model", displayName: null }] }}
        onModelChange={() => {}}
      />,
    );
    const select = screen.getByLabelText(/anthropic model/i) as HTMLSelectElement;
    expect(select.options.length).toBe(1);
    expect(screen.getByRole("option", { name: "only-model" })).toBeTruthy();
  });

  it("does not render a model dropdown for a CLI provider (no models entry)", () => {
    render(<ProviderSettings all={all} configured={["claude-code"]} onSave={() => {}} models={{}} />);
    expect(screen.queryByLabelText(/claude-code model/i)).toBeNull();
  });
});
