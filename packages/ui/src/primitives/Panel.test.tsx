import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { Panel } from "./Panel";

describe("Panel", () => {
  it("shows title and children when expanded", () => {
    render(<Panel title="Green Room" side="left" collapsed={false} onToggle={() => {}}>body</Panel>);
    expect(screen.getByText("Green Room")).toBeTruthy();
    expect(screen.getByText("body")).toBeTruthy();
  });

  it("hides children when collapsed", () => {
    render(<Panel title="Green Room" side="left" collapsed onToggle={() => {}}>body</Panel>);
    expect(screen.queryByText("body")).toBeNull();
  });

  it("calls onToggle when the toggle is clicked", async () => {
    const onToggle = vi.fn();
    render(<Panel title="Green Room" side="left" collapsed={false} onToggle={onToggle}>body</Panel>);
    await userEvent.click(screen.getByRole("button", { name: /collapse|toggle/i }));
    expect(onToggle).toHaveBeenCalledOnce();
  });
});
