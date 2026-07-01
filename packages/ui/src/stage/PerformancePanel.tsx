import type { JSX } from "react";
import type { ToolCallData, AgentStatus } from "../types";
import { ToolCallCard } from "./ToolCallCard";
import { StatusDot } from "../primitives/StatusDot";

export function PerformancePanel({
  status, toolCalls, files,
}: {
  status: AgentStatus;
  toolCalls: ToolCallData[];
  files: string[];
}): JSX.Element {
  return (
    <div className="flex flex-col gap-4 p-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-wider text-base-content/60">Agent</span>
        <StatusDot status={status} label={status} />
      </div>

      {toolCalls.length === 0 ? (
        <div data-testid="performance-empty" className="text-sm text-base-content/40">No tool calls yet.</div>
      ) : (
        <div className="flex flex-col gap-2">
          <span className="text-xs uppercase tracking-wider text-base-content/50">Tool calls</span>
          {toolCalls.map((c) => <ToolCallCard key={c.id} call={c} />)}
        </div>
      )}

      {files.length > 0 && (
        <div className="flex flex-col gap-1">
          <span className="text-xs uppercase tracking-wider text-base-content/50">Files</span>
          {files.map((f) => <span key={f} className="font-mono text-xs text-base-content/70">{f}</span>)}
        </div>
      )}
    </div>
  );
}
