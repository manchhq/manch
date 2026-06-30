import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { CompareView } from "./CompareView";

describe("CompareView", () => {
  it("renders one column per report plus the summary", () => {
    render(
      <CompareView
        reports={[
          { provider: "anthropic", text: "A says" },
          { provider: "claude-code", text: "B says" },
        ]}
        summary="they agree"
      />
    );
    expect(screen.getByText(/A says/)).toBeTruthy();
    expect(screen.getByText(/B says/)).toBeTruthy();
    expect(screen.getByText(/they agree/)).toBeTruthy();
    expect(screen.getAllByRole("article")).toHaveLength(2);
  });
});
