import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ScheduleForm } from "./ScheduleForm";

describe("ScheduleForm", () => {
  it("submits target/cadence/nextRun", () => {
    const onCreate = vi.fn();
    render(<ScheduleForm onCreate={onCreate} />);
    fireEvent.change(screen.getByLabelText(/target/i), {
      target: { value: "Discovery team" },
    });
    fireEvent.change(screen.getByLabelText(/next run/i), {
      target: { value: "2026-07-01T09:00" },
    });
    fireEvent.click(screen.getByRole("button", { name: /add schedule/i }));
    expect(onCreate).toHaveBeenCalledWith(
      expect.objectContaining({ target: "Discovery team", cadence: "daily" })
    );
  });
});
