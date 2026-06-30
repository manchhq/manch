import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { WorkspaceSettings } from "./WorkspaceSettings";

describe("WorkspaceSettings", () => {
  it("renames a workspace", () => {
    const onRename = vi.fn();
    render(<WorkspaceSettings workspaces={[{ id: "w1", name: "Old" }]} onRename={onRename} onDelete={() => {}} />);
    const input = screen.getByDisplayValue("Old");
    fireEvent.change(input, { target: { value: "New" } });
    fireEvent.click(screen.getByRole("button", { name: /save/i }));
    expect(onRename).toHaveBeenCalledWith("w1", "New");
  });
  it("deletes a workspace", () => {
    const onDelete = vi.fn();
    render(<WorkspaceSettings workspaces={[{ id: "w1", name: "Old" }]} onRename={() => {}} onDelete={onDelete} />);
    fireEvent.click(screen.getByRole("button", { name: /delete/i }));
    expect(onDelete).toHaveBeenCalledWith("w1");
  });
});
