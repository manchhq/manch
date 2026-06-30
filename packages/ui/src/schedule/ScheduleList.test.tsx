import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { ScheduleList } from "./ScheduleList";

describe("ScheduleList", () => {
  it("renders schedules", () => {
    render(<ScheduleList schedules={[{ id: "s1", target: "Discovery team", cadence: "daily", nextRun: "2026-07-01T09:00:00Z" }]} />);
    expect(screen.getByText("Discovery team")).toBeTruthy();
    expect(screen.getByText(/daily/)).toBeTruthy();
  });
});
