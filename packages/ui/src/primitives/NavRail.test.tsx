import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { NavRail } from "./NavRail";

const items = [
  { id: "chat", label: "Chat", glyph: "💬" },
  { id: "teams", label: "Teams", glyph: "👥" },
];

describe("NavRail", () => {
  it("marks the active item with aria-current", () => {
    render(<NavRail items={items} activeId="teams" onSelect={() => {}} />);
    expect(screen.getByRole("tab", { name: /Teams/ }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("tab", { name: /Chat/ }).getAttribute("aria-current")).toBeNull();
  });

  it("calls onSelect with the item id when clicked", () => {
    const onSelect = vi.fn();
    render(<NavRail items={items} activeId="chat" onSelect={onSelect} />);
    fireEvent.click(screen.getByRole("tab", { name: /Teams/ }));
    expect(onSelect).toHaveBeenCalledWith("teams");
  });
});
