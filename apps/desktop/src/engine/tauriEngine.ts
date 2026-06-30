import { sendPrompt } from "../lib/api";
import { isProvider } from "../lib/providers";
import type { StageEngine, StageEvent } from "./StageEngine";

export const tauriEngine: StageEngine = {
  async *send(provider: string, text: string): AsyncIterable<StageEvent> {
    try {
      if (!isProvider(provider)) {
        yield { kind: "error", message: `Unknown provider: ${provider}` };
        return;
      }
      const reply = await sendPrompt(provider, text);
      yield { kind: "token", text: reply };
      yield { kind: "done" };
    } catch (e) {
      yield { kind: "error", message: typeof e === "string" ? e : String(e) };
    }
  },
};
