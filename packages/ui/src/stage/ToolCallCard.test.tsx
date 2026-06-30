import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { ToolCallCard } from "./ToolCallCard";

describe("ToolCallCard", () => {
  it("shows the tool name, detail, and a status", () => {
    render(<ToolCallCard call={{ id: "t1", name: "read_file", status: "done", detail: "parser.rs" }} />);
    expect(screen.getByText("read_file")).toBeTruthy();
    expect(screen.getByText("parser.rs")).toBeTruthy();
    expect(screen.getByTestId("toolcall").getAttribute("data-status")).toBe("done");
  });
});
