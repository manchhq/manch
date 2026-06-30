import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { SettingsView } from "./SettingsView";

describe("SettingsView", () => {
  it("renders all three sections", () => {
    render(<SettingsView providers={<div>P</div>} theme={<div>T</div>} workspaces={<div>W</div>} />);
    expect(screen.getByText("P")).toBeTruthy();
    expect(screen.getByText("T")).toBeTruthy();
    expect(screen.getByText("W")).toBeTruthy();
  });
});
