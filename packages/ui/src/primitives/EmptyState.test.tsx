import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders title and description", () => {
    render(<EmptyState title="No teams yet" description="Create one to begin." />);
    expect(screen.getByText("No teams yet")).toBeTruthy();
    expect(screen.getByText("Create one to begin.")).toBeTruthy();
  });

  it("fires the action callback", () => {
    const onClick = vi.fn();
    render(<EmptyState title="Empty" action={{ label: "New", onClick }} />);
    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
