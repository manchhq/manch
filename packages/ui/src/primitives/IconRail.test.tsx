import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { IconRail } from "./IconRail";

describe("IconRail", () => {
  it("renders one button per item and fires onClick", async () => {
    const onClick = vi.fn();
    render(<IconRail items={[{ id: "new", glyph: "+", label: "New", onClick }]} />);
    const btn = screen.getByRole("button", { name: "New" });
    expect(btn.textContent).toContain("+");
    await userEvent.click(btn);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
