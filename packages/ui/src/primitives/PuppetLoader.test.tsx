import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { PuppetLoader } from "./PuppetLoader";

describe("PuppetLoader", () => {
  it("exposes an accessible status with the given label", () => {
    render(<PuppetLoader label="Thinking…" />);
    const status = screen.getByRole("status");
    expect(status).toBeTruthy();
    expect(status.getAttribute("aria-label")).toBe("Thinking…");
  });

  it("defaults the label when none is given", () => {
    render(<PuppetLoader />);
    expect(screen.getByRole("status").getAttribute("aria-label")).toBe("Loading");
  });
});
