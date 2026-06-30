import type { ToolCallData } from "@manch/ui";
import type { StageEvent } from "./StageEngine";

export interface LiveState {
  text: string;
  toolCalls: ToolCallData[];
}

export const emptyLive: LiveState = { text: "", toolCalls: [] };

export function applyEvent(state: LiveState, event: StageEvent): LiveState {
  switch (event.kind) {
    case "token":
      return { ...state, text: state.text + event.text };
    case "tool": {
      const existing = state.toolCalls.findIndex((c) => c.id === event.id);
      const call: ToolCallData = { id: event.id, name: event.name, status: event.status, detail: event.detail };
      const toolCalls = existing >= 0
        ? state.toolCalls.map((c, i) => (i === existing ? call : c))
        : [...state.toolCalls, call];
      return { ...state, toolCalls };
    }
    case "done":
    case "error":
      return state;
  }
}
