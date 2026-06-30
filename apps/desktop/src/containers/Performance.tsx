import { useAtomValue } from "jotai";
import { PerformancePanel } from "@manch/ui";
import {
  activeConversationAtom,
  agentStatusAtom,
  isStreamingAtom,
  liveToolCallsAtom,
} from "../store/atoms";

export default function Performance() {
  const convo = useAtomValue(activeConversationAtom);
  const status = useAtomValue(agentStatusAtom);
  const isStreaming = useAtomValue(isStreamingAtom);
  const live = useAtomValue(liveToolCallsAtom);

  const toolCalls = isStreaming ? live : (convo?.toolCalls ?? []);
  const files = Array.from(
    new Set(toolCalls.map((t) => t.detail).filter((d): d is string => !!d)),
  );

  return <PerformancePanel status={status} toolCalls={toolCalls} files={files} />;
}
