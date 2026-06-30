import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TeamDetail } from "./TeamDetail";

const base = {
  name: "Discovery", problem: "find precedent",
  members: [{ role: "Researcher", provider: "anthropic" }],
  capabilities: ["read_file", "search"],
};

describe("TeamDetail", () => {
  it("renders members and capabilities", () => {
    render(<TeamDetail {...base} onAssign={() => {}} />);
    expect(screen.getByText("Researcher")).toBeTruthy();
    expect(screen.getByText("read_file")).toBeTruthy();
  });
  it("assigns a task", () => {
    const onAssign = vi.fn();
    render(<TeamDetail {...base} onAssign={onAssign} />);
    fireEvent.change(screen.getByLabelText(/task/i), { target: { value: "summarize case" } });
    fireEvent.click(screen.getByRole("button", { name: /assign/i }));
    expect(onAssign).toHaveBeenCalledWith("summarize case");
  });
  it("renders a run timeline when a run is present", () => {
    const run = { task: "t", steps: [{ memberRole: "Researcher", detail: "did x", status: "done" as const }], result: "synthesized" };
    render(<TeamDetail {...base} run={run} onAssign={() => {}} />);
    expect(screen.getByText(/synthesized/)).toBeTruthy();
    expect(screen.getByText(/did x/)).toBeTruthy();
  });
});
