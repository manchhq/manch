import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";
import type { MessageData, ToolCallData, AgentStatus } from "@manch/ui";

export interface Conversation {
  id: string;
  title: string;
  messages: MessageData[];
  toolCalls: ToolCallData[];
}

let counter = 0;
export function newConversation(title = "New conversation"): Conversation {
  counter += 1;
  const id = `c_${Date.now()}_${counter}`;
  return { id, title, messages: [], toolCalls: [] };
}

export const conversationsAtom = atomWithStorage<Conversation[]>("manch.conversations", []);
export const activeIdAtom = atomWithStorage<string | null>("manch.activeId", null);
export const leftCollapsedAtom = atomWithStorage<boolean>("manch.leftCollapsed", false);
export const rightCollapsedAtom = atomWithStorage<boolean>("manch.rightCollapsed", false);

export const streamingTextAtom = atom<string>("");
export const liveToolCallsAtom = atom<ToolCallData[]>([]);
export const agentStatusAtom = atom<AgentStatus>("idle");
export const isStreamingAtom = atom<boolean>(false);

export const activeConversationAtom = atom((get) => {
  const id = get(activeIdAtom);
  return get(conversationsAtom).find((c) => c.id === id) ?? null;
});

export const settingsOpenAtom = atom<boolean>(false);
