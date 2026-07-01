import type { JSX } from "react";
import type { MessageData } from "../types";
import { Message } from "./Message";
import { Spotlight } from "../primitives/Spotlight";

export function Transcript({
  messages, streamingText = "", isStreaming = false,
}: {
  messages: MessageData[];
  streamingText?: string;
  isStreaming?: boolean;
}): JSX.Element {
  if (messages.length === 0 && !isStreaming) {
    return (
      <div data-testid="transcript-empty" className="flex h-full items-center justify-center text-center text-base-content/70">
        <p>The stage is set.<br />Send a prompt to begin the performance.</p>
      </div>
    );
  }

  const live: MessageData[] = isStreaming
    ? [...messages, { id: "__live__", role: "agent", text: streamingText || "…" }]
    : messages;

  return (
    <div className="flex flex-col gap-4 p-4">
      {live.map((m, i) => (
        <Spotlight key={m.id} active={i === live.length - 1}>
          <Message message={m} />
        </Spotlight>
      ))}
    </div>
  );
}
