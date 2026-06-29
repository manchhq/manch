import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Message } from "./Message";

describe("Message", () => {
  it("renders agent markdown (bold) and marks the role", () => {
    render(<Message message={{ id: "1", role: "agent", text: "hello **world**" }} />);
    const root = screen.getByTestId("message");
    expect(root.getAttribute("data-role")).toBe("agent");
    expect(screen.getByText("world").tagName.toLowerCase()).toBe("strong");
  });

  it("renders user text on the end side", () => {
    render(<Message message={{ id: "2", role: "user", text: "hi" }} />);
    expect(screen.getByTestId("message").className).toContain("chat-end");
  });
});
