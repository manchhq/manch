import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { Button } from "./Button";

describe("Button", () => {
  it("renders its label", () => {
    render(<Button label="Click me" onClick={() => {}} />);
    expect(screen.getByRole("button", { name: "Click me" })).toBeInTheDocument();
  });

  it("calls onClick when pressed", async () => {
    const onClick = vi.fn();
    render(<Button label="Go" onClick={onClick} />);
    await userEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("applies the daisyUI variant class", () => {
    render(<Button label="Primary" variant="primary" onClick={() => {}} />);
    expect(screen.getByRole("button", { name: "Primary" })).toHaveClass("btn-primary");
  });

  it("does not call onClick when disabled", async () => {
    const onClick = vi.fn();
    render(<Button label="Save" onClick={onClick} disabled />);
    const btn = screen.getByRole("button", { name: "Save" });
    expect(btn).toBeDisabled();
    await userEvent.click(btn);
    expect(onClick).not.toHaveBeenCalled();
  });
});
