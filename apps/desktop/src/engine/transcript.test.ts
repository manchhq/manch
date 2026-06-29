import { describe, it, expect } from "vitest";
import { applyEvent, emptyLive } from "./transcript";

describe("applyEvent", () => {
  it("accumulates token text", () => {
    let s = emptyLive;
    s = applyEvent(s, { kind: "token", text: "Hello " });
    s = applyEvent(s, { kind: "token", text: "world" });
    expect(s.text).toBe("Hello world");
  });

  it("adds a tool call and updates its status by id", () => {
    let s = emptyLive;
    s = applyEvent(s, { kind: "tool", id: "t1", name: "read_file", status: "running", detail: "parser.rs" });
    expect(s.toolCalls).toHaveLength(1);
    s = applyEvent(s, { kind: "tool", id: "t1", name: "read_file", status: "done", detail: "parser.rs" });
    expect(s.toolCalls).toHaveLength(1);
    expect(s.toolCalls[0].status).toBe("done");
  });
});
