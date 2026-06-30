import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamComposer } from "./TeamComposer";

const providers = [{ id: "anthropic", label: "Anthropic" }];

describe("TeamComposer", () => {
  it("submits an auto-compose team from a problem statement", () => {
    const onCreate = vi.fn();
    render(<TeamComposer providers={providers} onCreate={onCreate} />);
    fireEvent.change(screen.getByLabelText(/problem/i), { target: { value: "find precedent" } });
    fireEvent.click(screen.getByRole("button", { name: /create team/i }));
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ problem: "find precedent", auto: true }));
  });

  it("nudges to settings and disables provider selection when no providers", () => {
    const onConfigureProviders = vi.fn();
    render(<TeamComposer providers={[]} onCreate={() => {}} onConfigureProviders={onConfigureProviders} />);
    fireEvent.click(screen.getByRole("button", { name: /add an ai provider/i }));
    expect(onConfigureProviders).toHaveBeenCalledOnce();
  });
});
