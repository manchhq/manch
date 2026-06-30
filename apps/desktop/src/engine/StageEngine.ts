import type { ToolCallData } from "@manch/ui";

export type StageEvent =
  | { kind: "token"; text: string }
  | { kind: "tool"; id: string; name: string; status: ToolCallData["status"]; detail?: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

export interface StageEngine {
  send(provider: string, text: string): AsyncIterable<StageEvent>;
}
