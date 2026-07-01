import { describe, it, expect, vi } from "vitest";

// vi.hoisted ensures these are available when vi.mock's factory runs (which is
// hoisted to the top of the module before any imports are resolved).
const { invoke, FakeChannel } = vi.hoisted(() => {
  // A fake Tauri Channel: captures the assigned onmessage, lets the test push
  // StreamEvents, and records the invoke it was passed to.
  class FakeChannel {
    onmessage: ((m: unknown) => void) | null = null;
  }
  return { invoke: vi.fn(), FakeChannel };
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  Channel: FakeChannel,
}));

import { tauriEngine } from "./tauriEngine";
import type { StageEvent } from "./StageEngine";

type FakeChannelInstance = InstanceType<typeof FakeChannel>;

async function collect(
  provider: string,
  text: string,
  script: (ch: FakeChannelInstance) => void,
): Promise<StageEvent[]> {
  invoke.mockImplementation((_cmd: string, args: { channel: FakeChannelInstance }) => {
    // Drive the channel on the next tick, then resolve the command.
    queueMicrotask(() => script(args.channel));
    return Promise.resolve();
  });
  const out: StageEvent[] = [];
  for await (const ev of tauriEngine.send(provider, text)) out.push(ev);
  return out;
}

describe("tauriEngine", () => {
  it("maps StreamEvents to StageEvents in order and terminates on done", async () => {
    const events = await collect("anthropic", "hi", (ch) => {
      ch.onmessage?.({ kind: "token", text: "He" });
      ch.onmessage?.({ kind: "token", text: "llo" });
      ch.onmessage?.({ kind: "tool", id: "t1", name: "read", status: "running", detail: "x.rs" });
      ch.onmessage?.({ kind: "tool", id: "t1", name: "read", status: "done", detail: "x.rs" });
      ch.onmessage?.({ kind: "done" });
    });
    expect(events).toEqual([
      { kind: "token", text: "He" },
      { kind: "token", text: "llo" },
      { kind: "tool", id: "t1", name: "read", status: "running", detail: "x.rs" },
      { kind: "tool", id: "t1", name: "read", status: "done", detail: "x.rs" },
      { kind: "done" },
    ]);
    expect(invoke).toHaveBeenCalledWith(
      "send_prompt_stream",
      expect.objectContaining({ provider: "anthropic", text: "hi" }),
    );
  });

  it("surfaces an error event and stops", async () => {
    const events = await collect("anthropic", "hi", (ch) => {
      ch.onmessage?.({ kind: "error", message: "boom" });
    });
    expect(events).toEqual([{ kind: "error", message: "boom" }]);
  });

  it("maps tool detail null to undefined", async () => {
    const events = await collect("anthropic", "hi", (ch) => {
      ch.onmessage?.({ kind: "tool", id: "t1", name: "read", status: "running", detail: null });
      ch.onmessage?.({ kind: "done" });
    });
    expect(events[0]).toEqual({ kind: "tool", id: "t1", name: "read", status: "running", detail: undefined });
  });

  it("surfaces a rejected command invoke as an error event", async () => {
    invoke.mockImplementation(() => Promise.reject("backend boom"));
    const out: StageEvent[] = [];
    for await (const ev of tauriEngine.send("anthropic", "hi")) out.push(ev);
    expect(out).toEqual([{ kind: "error", message: "backend boom" }]);
  });
});
