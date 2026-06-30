import { useAtomValue, useAtom } from "jotai";
import { useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { StageHeader, Transcript, Composer, CompareView } from "@manch/ui";
import { ALL_PROVIDERS } from "../lib/providers";
import {
  activeConversationAtom,
  agentStatusAtom,
  compareProvidersAtom,
  isStreamingAtom,
  streamingTextAtom,
} from "../store/atoms";
import { useSend } from "../data/useSend";
import { useCrossVerify, useConfiguredProviders } from "../data/queries";
import { mockEngine } from "../engine/mockEngine";

export default function Stage() {
  const convo = useAtomValue(activeConversationAtom);
  const status = useAtomValue(agentStatusAtom);
  const isStreaming = useAtomValue(isStreamingAtom);
  const streamingText = useAtomValue(streamingTextAtom);
  const { send, busy } = useSend(mockEngine);
  const [provider, setProvider] = useState(ALL_PROVIDERS[0].id);
  const [input, setInput] = useState("");
  const [compareProviders, setCompareProviders] = useAtom(compareProvidersAtom);
  const crossVerify = useCrossVerify();
  const configured = useConfiguredProviders();
  const navigate = useNavigate();

  const isCompareMode = compareProviders.length > 1;
  // Only gate after the query has settled — undefined means still loading (don't block)
  const noProviders = configured.data !== undefined && configured.data.length === 0;

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
      {/* Compare provider multi-select (inline control — purely additive, gated on length > 1) */}
      <div className="flex flex-wrap items-center gap-3 border-b border-base-300 px-4 py-2 text-sm">
        <span className="font-medium text-base-content/60">Compare:</span>
        {ALL_PROVIDERS.map((p) => (
          <label key={p.id} className="flex cursor-pointer items-center gap-1">
            <input
              type="checkbox"
              className="checkbox checkbox-xs"
              checked={compareProviders.includes(p.id)}
              onChange={(e) => {
                if (e.target.checked) {
                  setCompareProviders([...compareProviders, p.id]);
                } else {
                  setCompareProviders(compareProviders.filter((id) => id !== p.id));
                }
              }}
            />
            {p.label}
          </label>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {isCompareMode && crossVerify.data ? (
          <CompareView
            reports={crossVerify.data.reports}
            summary={crossVerify.data.summary}
          />
        ) : (
          <Transcript
            messages={convo.messages}
            isStreaming={isStreaming}
            streamingText={streamingText}
          />
        )}
      </div>
      {noProviders && (
        <div role="alert" className="alert alert-warning mx-4 mb-2 flex items-center gap-2 text-sm">
          <span>No AI provider configured.</span>
          <button
            type="button"
            className="btn btn-sm btn-outline"
            onClick={() => navigate({ to: "/settings" })}
          >
            Configure a provider
          </button>
        </div>
      )}
      <Composer
        value={input}
        busy={noProviders || (isCompareMode ? crossVerify.isPending : busy)}
        onChange={setInput}
        onSend={() => {
          if (noProviders) return;
          if (isCompareMode) {
            crossVerify.mutate({ providers: compareProviders, text: input });
          } else {
            send(provider, input);
          }
          setInput("");
        }}
      />
    </div>
  );
}
