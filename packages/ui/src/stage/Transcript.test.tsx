import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Transcript } from "./Transcript";

describe("Transcript", () => {
  it("shows an empty state with no messages", () => {
    render(<Transcript messages={[]} />);
    expect(screen.getByTestId("transcript-empty")).toBeTruthy();
  });

  it("renders messages and spotlights the last", () => {
    render(<Transcript messages={[
      { id: "1", role: "user", text: "hi" },
      { id: "2", role: "agent", text: "hello" },
    ]} />);
    const spots = screen.getAllByTestId("spotlight");
    expect(spots[spots.length - 1].getAttribute("data-active")).toBe("true");
  });

  it("renders the streaming text as a live agent message", () => {
    render(<Transcript messages={[{ id: "1", role: "user", text: "hi" }]} isStreaming streamingText="typing" />);
    expect(screen.getByText("typing")).toBeTruthy();
  });
});
