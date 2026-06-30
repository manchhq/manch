import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamList } from "./TeamList";

const teams = [
  { id: "tm_1", name: "Discovery", problem: "p1", memberCount: 2 },
  { id: "tm_2", name: "Drafting", problem: "p2", memberCount: 1 },
];

describe("TeamList", () => {
  it("renders all teams and fires onNew", () => {
    const onNew = vi.fn();
    render(<TeamList teams={teams} onOpen={() => {}} onNew={onNew} />);
    expect(screen.getByText("Discovery")).toBeTruthy();
    expect(screen.getByText("Drafting")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /New team/ }));
    expect(onNew).toHaveBeenCalledOnce();
  });
});
