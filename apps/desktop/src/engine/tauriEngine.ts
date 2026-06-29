import { sendPrompt } from "../lib/api";
import type { StageEngine, StageEvent } from "./StageEngine";

export const tauriEngine: StageEngine = {
  async *send(provider: string, text: string): AsyncIterable<StageEvent> {
    try {
      const reply = await sendPrompt(provider as never, text);
      yield { kind: "token", text: reply };
      yield { kind: "done" };
    } catch (e) {
      yield { kind: "error", message: typeof e === "string" ? e : String(e) };
    }
  },
};
