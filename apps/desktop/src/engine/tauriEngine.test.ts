import { describe, it, expect, vi, beforeEach } from "vitest";
import type { StageEvent } from "./StageEngine";

vi.mock("../lib/api", () => ({ sendPrompt: vi.fn() }));
import { sendPrompt } from "../lib/api";
import { tauriEngine } from "./tauriEngine";

describe("tauriEngine", () => {
  beforeEach(() => vi.resetAllMocks());

  it("emits the reply as one token then done", async () => {
    (sendPrompt as unknown as ReturnType<typeof vi.fn>).mockResolvedValue("New Delhi.");
    const events: StageEvent[] = [];
    for await (const e of tauriEngine.send("anthropic", "capital of India?")) events.push(e);
    expect(events[0]).toEqual({ kind: "token", text: "New Delhi." });
    expect(events[1]).toEqual({ kind: "done" });
  });

  it("emits error when the command rejects", async () => {
    (sendPrompt as unknown as ReturnType<typeof vi.fn>).mockRejectedValue("boom");
    const events: StageEvent[] = [];
    for await (const e of tauriEngine.send("anthropic", "x")) events.push(e);
    expect(events[0]).toEqual({ kind: "error", message: "boom" });
  });
});
