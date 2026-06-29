import { useAtom, useSetAtom } from "jotai";
import { useCallback } from "react";
import type { MessageData } from "@manch/ui";
import type { StageEngine } from "../engine/StageEngine";
import { applyEvent, emptyLive, type LiveState } from "../engine/transcript";
import {
  activeIdAtom, agentStatusAtom, conversationsAtom, isStreamingAtom,
  liveToolCallsAtom, streamingTextAtom,
} from "../store/atoms";

export function useSend(engine: StageEngine) {
  const setConversations = useSetAtom(conversationsAtom);
  const [activeId] = useAtom(activeIdAtom);
  const [isStreaming, setIsStreaming] = useAtom(isStreamingAtom);
  const setStreamingText = useSetAtom(streamingTextAtom);
  const setLiveToolCalls = useSetAtom(liveToolCallsAtom);
  const setStatus = useSetAtom(agentStatusAtom);

  const appendMessage = useCallback((id: string, msg: MessageData) => {
    setConversations((cs) => cs.map((c) => c.id === id ? { ...c, messages: [...c.messages, msg] } : c));
  }, [setConversations]);

  const send = useCallback((provider: string, text: string) => {
    if (!activeId || isStreaming || text.trim() === "") return;
    appendMessage(activeId, { id: `m_${Date.now()}`, role: "user", text });
    setStreamingText(""); setLiveToolCalls([]); setStatus("busy"); setIsStreaming(true);

    void (async () => {
      let live: LiveState = emptyLive;
      let errored = false;
      try {
        for await (const ev of engine.send(provider, text)) {
          live = applyEvent(live, ev);
          if (ev.kind === "token") setStreamingText(live.text);
          if (ev.kind === "tool") setLiveToolCalls(live.toolCalls);
          if (ev.kind === "error") { setStatus("error"); errored = true; break; }
        }
        if (!errored) {
          appendMessage(activeId, { id: `m_${Date.now()}_a`, role: "agent", text: live.text || "(no output)" });
          setConversations((cs) => cs.map((c) => c.id === activeId ? { ...c, toolCalls: live.toolCalls } : c));
          setStatus("done");
        }
      } catch {
        setStatus("error");
      } finally {
        setIsStreaming(false);
        setStreamingText("");
        setLiveToolCalls([]);
      }
    })();
  }, [activeId, isStreaming, engine, appendMessage, setConversations, setStreamingText, setLiveToolCalls, setStatus, setIsStreaming]);

  return { send, busy: isStreaming };
}
