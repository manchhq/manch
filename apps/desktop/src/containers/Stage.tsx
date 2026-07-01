import { useAtomValue, useAtom, useSetAtom } from "jotai";
import { useEffect, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { StageHeader, Transcript, Composer, CompareView, EmptyState, PuppetLoader } from "@manch/ui";
import { ALL_PROVIDERS, type Provider } from "../lib/providers";
import {
  activeConversationAtom,
  agentStatusAtom,
  compareProvidersAtom,
  isStreamingAtom,
  streamingTextAtom,
  conversationsAtom,
  activeIdAtom,
  newConversation,
} from "../store/atoms";
import { useSend } from "../data/useSend";
import { useCrossVerify, useConfiguredProviders } from "../data/queries";
import { tauriEngine } from "../engine/tauriEngine";

export default function Stage() {
  const convo = useAtomValue(activeConversationAtom);
  const status = useAtomValue(agentStatusAtom);
  const isStreaming = useAtomValue(isStreamingAtom);
  const streamingText = useAtomValue(streamingTextAtom);
  const { send, busy } = useSend(tauriEngine);
  const [provider, setProvider] = useState(ALL_PROVIDERS[0].id);
  const [input, setInput] = useState("");
  const [compareProviders, setCompareProviders] = useAtom(compareProvidersAtom);
  const crossVerify = useCrossVerify();
  const configured = useConfiguredProviders();
  const navigate = useNavigate();
  const setConversations = useSetAtom(conversationsAtom);
  const setActiveId = useSetAtom(activeIdAtom);

  // `crossVerify.data` is component-held mutation state, not scoped to a
  // conversation. Clear it when the compare set or the active conversation
  // changes so a stale CompareView doesn't flash on re-entry/switch.
  const resetCrossVerify = crossVerify.reset;
  const convoId = convo?.id;
  useEffect(() => {
    resetCrossVerify();
  }, [compareProviders, convoId, resetCrossVerify]);

  const isCompareMode = compareProviders.length > 1;
  // Only gate after the query has settled — undefined means still loading (don't block)
  const noProviders = configured.data !== undefined && configured.data.length === 0;
  // Compare only offers configured AIs (spec: multi-select of configured providers).
  const compareOptions = ALL_PROVIDERS.filter((p) =>
    (configured.data ?? []).includes(p.id as Provider),
  );

  if (!convo) {
    return (
      <EmptyState
        glyph="💬"
        title="No conversation yet"
        description="Start a new conversation to get going."
        action={{
          label: "New conversation",
          onClick: () => {
            const c = newConversation();
            setConversations((cs) => [c, ...cs]);
            setActiveId(c.id);
          },
        }}
      />
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
        {compareOptions.map((p) => (
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
      {isStreaming && (
        <div className="flex items-center justify-center py-2">
          <PuppetLoader size={32} label="Streaming…" />
        </div>
      )}
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
