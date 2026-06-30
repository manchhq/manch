import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ThemePicker } from "./ThemePicker";

describe("ThemePicker", () => {
  it("marks the active theme as checked", () => {
    render(<ThemePicker themes={["dark", "light"]} active="light" onSelect={() => {}} />);
    const light = screen.getByRole("radio", { name: "light" }) as HTMLInputElement;
    expect(light.checked).toBe(true);
  });

  it("calls onSelect when a theme is chosen", () => {
    const onSelect = vi.fn();
    render(<ThemePicker themes={["dark", "light"]} active="dark" onSelect={onSelect} />);
    fireEvent.click(screen.getByRole("radio", { name: "light" }));
    expect(onSelect).toHaveBeenCalledWith("light");
  });
});
