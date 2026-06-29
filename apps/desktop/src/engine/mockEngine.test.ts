import { describe, it, expect } from "vitest";
import { mockEngine } from "./mockEngine";
import type { StageEvent } from "./StageEngine";

describe("mockEngine", () => {
  it("streams tokens, a tool call (running→done), and ends with done", async () => {
    const events: StageEvent[] = [];
    for await (const e of mockEngine.send("claude-code", "refactor the parser")) events.push(e);

    expect(events.some((e) => e.kind === "token")).toBe(true);

    type ToolEvent = Extract<StageEvent, { kind: "tool" }>;
    const tools = events.filter((e): e is ToolEvent => e.kind === "tool");
    const running = tools.find((e) => e.status === "running");
    const done = tools.find((e) => e.status === "done");
    expect(running).toBeDefined();
    expect(done).toBeDefined();
    expect(running?.id).toBe(done?.id); // running→done must share the same tool id

    expect(events[events.length - 1].kind).toBe("done");
  });
});
