import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { StageHeader } from "./StageHeader";

const providers = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];

describe("StageHeader", () => {
  it("lists providers and reflects the active one", () => {
    render(<StageHeader providers={providers} activeProvider="claude-code" onProviderChange={() => {}} status="idle" />);
    expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("claude-code");
  });

  it("fires onProviderChange on selection", async () => {
    const onChange = vi.fn();
    render(<StageHeader providers={providers} activeProvider="anthropic" onProviderChange={onChange} status="idle" />);
    await userEvent.selectOptions(screen.getByRole("combobox"), "claude-code");
    expect(onChange).toHaveBeenCalledWith("claude-code");
  });
});
