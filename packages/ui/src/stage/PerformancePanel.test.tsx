import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { PerformancePanel } from "./PerformancePanel";

describe("PerformancePanel", () => {
  it("shows status, tool calls, and files", () => {
    render(<PerformancePanel
      status="busy"
      toolCalls={[{ id: "t1", name: "read_file", status: "done", detail: "parser.rs" }]}
      files={["parser.rs", "lexer.rs"]}
    />);
    expect(screen.getAllByRole("status")[0].getAttribute("data-status")).toBe("busy");
    expect(screen.getByText("read_file")).toBeTruthy();
    expect(screen.getByText("lexer.rs")).toBeTruthy();
  });

  it("shows an idle empty state with no tool calls", () => {
    render(<PerformancePanel status="idle" toolCalls={[]} files={[]} />);
    expect(screen.getByTestId("performance-empty")).toBeTruthy();
  });
});
