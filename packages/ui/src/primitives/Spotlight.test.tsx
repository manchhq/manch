import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Spotlight } from "./Spotlight";

describe("Spotlight", () => {
  it("marks the wrapper active", () => {
    render(<Spotlight active><span>lit</span></Spotlight>);
    expect(screen.getByTestId("spotlight").getAttribute("data-active")).toBe("true");
  });
  it("is inactive by default state passed", () => {
    render(<Spotlight active={false}><span>dim</span></Spotlight>);
    expect(screen.getByTestId("spotlight").getAttribute("data-active")).toBe("false");
  });
});
