import { useAtomValue } from "jotai";
import { useState } from "react";
import { StageHeader, Transcript, Composer } from "@manch/ui";
import { ALL_PROVIDERS } from "../lib/providers";
import {
  activeConversationAtom,
  agentStatusAtom,
  isStreamingAtom,
  streamingTextAtom,
} from "../store/atoms";
import { useSend } from "../data/useSend";
import { mockEngine } from "../engine/mockEngine";

export default function Stage() {
  const convo = useAtomValue(activeConversationAtom);
  const status = useAtomValue(agentStatusAtom);
  const isStreaming = useAtomValue(isStreamingAtom);
  const streamingText = useAtomValue(streamingTextAtom);
  const { send, busy } = useSend(mockEngine);
  const [provider, setProvider] = useState(ALL_PROVIDERS[0].id);
  const [input, setInput] = useState("");

  if (!convo) {
    return (
      <div className="flex h-full items-center justify-center text-base-content/50">
        Select or start a conversation.
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <StageHeader
        providers={ALL_PROVIDERS}
        activeProvider={provider}
        onProviderChange={setProvider}
        status={status}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <Transcript
          messages={convo.messages}
          isStreaming={isStreaming}
          streamingText={streamingText}
        />
      </div>
      <Composer
        value={input}
        busy={busy}
        onChange={setInput}
        onSend={() => {
          send(provider, input);
          setInput("");
        }}
      />
    </div>
  );
}
