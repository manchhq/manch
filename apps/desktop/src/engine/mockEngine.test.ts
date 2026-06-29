import { describe, it, expect } from "vitest";
import { mockEngine } from "./mockEngine";
import type { StageEvent } from "./StageEngine";

describe("mockEngine", () => {
  it("streams tokens, a tool call (running→done), and ends with done", async () => {
    const events: StageEvent[] = [];
    for await (const e of mockEngine.send("claude-code", "refactor the parser")) events.push(e);

    expect(events.some((e) => e.kind === "token")).toBe(true);
    const tools = events.filter((e) => e.kind === "tool");
    expect(tools.some((e) => e.kind === "tool" && e.status === "running")).toBe(true);
    expect(tools.some((e) => e.kind === "tool" && e.status === "done")).toBe(true);
    expect(events[events.length - 1].kind).toBe("done");
  });
});
