import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { GreenRoomView } from "./GreenRoomView";

const convos = [{ id: "a", title: "Refactor auth" }, { id: "b", title: "Draft README" }];

describe("GreenRoomView", () => {
  it("lists conversations and marks the active one", () => {
    render(<GreenRoomView conversations={convos} activeId="b" onSelect={() => {}} onNew={() => {}} onOpenSettings={() => {}} />);
    expect(screen.getByText("Draft README").closest("[data-active]")!.getAttribute("data-active")).toBe("true");
  });

  it("fires onSelect, onNew, onOpenSettings", async () => {
    const onSelect = vi.fn(), onNew = vi.fn(), onOpenSettings = vi.fn();
    render(<GreenRoomView conversations={convos} activeId="a" onSelect={onSelect} onNew={onNew} onOpenSettings={onOpenSettings} />);
    await userEvent.click(screen.getByText("Draft README"));
    expect(onSelect).toHaveBeenCalledWith("b");
    await userEvent.click(screen.getByRole("button", { name: /new/i }));
    expect(onNew).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: /keys|settings/i }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
  });
});
