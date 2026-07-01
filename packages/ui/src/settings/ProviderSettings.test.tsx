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
});
