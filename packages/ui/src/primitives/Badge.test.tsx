import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Badge } from "./Badge";

describe("Badge", () => {
  it("renders children", () => {
    render(<Badge>read_file</Badge>);
    expect(screen.getByText("read_file")).toBeTruthy();
  });

  it("applies the tone class", () => {
    render(<Badge tone="accent">x</Badge>);
    expect(screen.getByText("x").className).toContain("badge-accent");
  });
});
