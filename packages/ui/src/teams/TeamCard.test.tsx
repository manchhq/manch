import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamCard } from "./TeamCard";

const team = { id: "tm_1", name: "Discovery", problem: "find precedent", memberCount: 3 };

describe("TeamCard", () => {
  it("shows name, problem, and member count", () => {
    render(<TeamCard team={team} onOpen={() => {}} />);
    expect(screen.getByText("Discovery")).toBeTruthy();
    expect(screen.getByText(/find precedent/)).toBeTruthy();
    expect(screen.getByText(/3/)).toBeTruthy();
  });
  it("opens on click", () => {
    const onOpen = vi.fn();
    render(<TeamCard team={team} onOpen={onOpen} />);
    fireEvent.click(screen.getByRole("button", { name: /Discovery/ }));
    expect(onOpen).toHaveBeenCalledWith("tm_1");
  });
});
