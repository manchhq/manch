import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { SettingsForm } from "./SettingsForm";

const providers = [{ id: "anthropic", label: "Anthropic" }];

describe("SettingsForm", () => {
  it("saves provider + key", async () => {
    const onSave = vi.fn();
    render(<SettingsForm providers={providers} onSave={onSave} />);
    await userEvent.type(screen.getByLabelText(/api key/i), "sk-test");
    await userEvent.click(screen.getByRole("button", { name: /save/i }));
    expect(onSave).toHaveBeenCalledWith("anthropic", "sk-test");
  });

  it("renders an error", () => {
    render(<SettingsForm providers={providers} onSave={() => {}} error="bad key" />);
    expect(screen.getByText("bad key")).toBeTruthy();
  });
});
