import { invoke, Channel } from "@tauri-apps/api/core";
import { isProvider } from "../lib/providers";
import type { StreamEvent } from "../data/bindings";
import type { StageEngine, StageEvent } from "./StageEngine";

/** Map a wire StreamEvent onto the internal StageEvent (near-identity; null→undefined). */
function toStageEvent(e: StreamEvent): StageEvent {
  switch (e.kind) {
    case "token":
      return { kind: "token", text: e.text };
    case "tool":
      return { kind: "tool", id: e.id, name: e.name, status: e.status as "running" | "done" | "error", detail: e.detail ?? undefined };
    case "done":
      return { kind: "done" };
    case "error":
      return { kind: "error", message: e.message };
  }
}

export const tauriEngine: StageEngine = {
  async *send(provider: string, text: string): AsyncIterable<StageEvent> {
    if (!isProvider(provider)) {
      yield { kind: "error", message: `Unknown provider: ${provider}` };
      return;
    }

    // Buffer channel messages into a queue the generator drains; resolve a
    // pending waiter as each event arrives. Terminates on done/error.
    const queue: StageEvent[] = [];
    let notify: (() => void) | null = null;
    let finished = false;

    const channel = new Channel<StreamEvent>();
    channel.onmessage = (msg) => {
      const ev = toStageEvent(msg);
      queue.push(ev);
      if (ev.kind === "done" || ev.kind === "error") finished = true;
      notify?.();
    };

    const done = invoke("send_prompt_stream", { provider, text, channel })
      .then(() => {
        // Both agents always emit a terminal done/error over the channel as
        // their LAST message, so `finished` is normally already set here. As a
        // safety net against a future Rust path that returns without a terminal
        // event, wake the drain loop on the NEXT macrotask — after any
        // already-enqueued channel messages have been delivered and processed
        // (so this never truncates in-flight output) — and let it terminate.
        setTimeout(() => {
          if (!finished) {
            finished = true;
            notify?.();
          }
        }, 0);
      })
      .catch((e: unknown): void => {
        queue.push({ kind: "error", message: typeof e === "string" ? e : String(e) });
        finished = true;
        notify?.();
      });

    // Drain until a terminal event has been yielded.
    for (;;) {
      while (queue.length > 0) {
        const ev = queue.shift() as StageEvent;
        yield ev;
        if (ev.kind === "done" || ev.kind === "error") {
          await done;
          return;
        }
      }
      if (finished && queue.length === 0) {
        await done;
        return;
      }
      await new Promise<void>((r) => {
        notify = r;
      });
      notify = null;
    }
  },
};
