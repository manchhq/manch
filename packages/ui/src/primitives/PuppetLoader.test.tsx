import { render, screen, within } from "@testing-library/react";
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

  it("hides the decorative svg from assistive tech", () => {
    const { container } = render(<PuppetLoader />);
    const svg = container.querySelector("svg");
    expect(svg).toBeTruthy();
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
  });

  it("renders both variants without error and keeps the status role", () => {
    const male = render(<PuppetLoader variant="male" label="M" />);
    expect(within(male.container).getByRole("status").getAttribute("aria-label")).toBe("M");
    const female = render(<PuppetLoader variant="female" label="F" />);
    expect(within(female.container).getByRole("status").getAttribute("aria-label")).toBe("F");
  });

  it("uses daisyUI semantic color classes (no flat single color)", () => {
    const { container } = render(<PuppetLoader variant="male" />);
    const svgHtml = container.querySelector("svg")?.outerHTML ?? "";
    expect(svgHtml).toContain("fill-primary");
    expect(svgHtml).toContain("fill-secondary");
    expect(svgHtml).toContain("fill-accent");
    // no hardcoded colors
    expect(svgHtml).not.toMatch(/#[0-9a-fA-F]{3,6}\b/);
  });
});
