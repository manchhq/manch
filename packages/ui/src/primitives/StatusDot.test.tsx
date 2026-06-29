import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { StatusDot } from "./StatusDot";

describe("StatusDot", () => {
  it("renders the label and a busy indicator", () => {
    render(<StatusDot status="busy" label="thinking" />);
    expect(screen.getByText("thinking")).toBeTruthy();
    expect(screen.getByRole("status").getAttribute("data-status")).toBe("busy");
  });

  it("defaults to no label", () => {
    render(<StatusDot status="idle" />);
    expect(screen.getByRole("status").getAttribute("data-status")).toBe("idle");
  });
});
