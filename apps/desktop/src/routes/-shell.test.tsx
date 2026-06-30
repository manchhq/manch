import { render, screen } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { NavRail } from "@manch/ui";

// The shell's NavRail is the navigation contract; full router rendering is covered e2e.
describe("app shell nav", () => {
  beforeEach(() => localStorage.clear());
  it("renders the five sections", () => {
    const items = [
      { id: "/chat", label: "Chat", glyph: "💬" },
      { id: "/teams", label: "Teams", glyph: "👥" },
      { id: "/schedule", label: "Schedule", glyph: "📅" },
      { id: "/search", label: "Search", glyph: "🔍" },
      { id: "/settings", label: "Settings", glyph: "⚙️" },
    ];
    render(<NavRail items={items} activeId="/chat" onSelect={() => {}} />);
    expect(screen.getAllByRole("tab")).toHaveLength(5);
  });
});
