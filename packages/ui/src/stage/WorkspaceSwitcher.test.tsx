import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

const ws = [
  { id: "w1", name: "Legal research" },
  { id: "w2", name: "Health" },
];

describe("WorkspaceSwitcher", () => {
  it("shows the active workspace name on the trigger", () => {
    render(<WorkspaceSwitcher workspaces={ws} activeId="w2" onSelect={() => {}} onCreate={() => {}} />);
    expect(screen.getByRole("button", { name: /Health/ })).toBeTruthy();
  });

  it("selects a workspace", () => {
    const onSelect = vi.fn();
    render(<WorkspaceSwitcher workspaces={ws} activeId="w1" onSelect={onSelect} onCreate={() => {}} />);
    fireEvent.click(screen.getByText("Health"));
    expect(onSelect).toHaveBeenCalledWith("w2");
  });

  it("fires onCreate", () => {
    const onCreate = vi.fn();
    render(<WorkspaceSwitcher workspaces={ws} activeId="w1" onSelect={() => {}} onCreate={onCreate} />);
    fireEvent.click(screen.getByRole("button", { name: /New workspace/ }));
    expect(onCreate).toHaveBeenCalledOnce();
  });
});
