import type { ToolCallData, AgentStatus } from "../types";
import { StatusDot } from "../primitives/StatusDot";
import { Badge } from "../primitives/Badge";

const AS: Record<ToolCallData["status"], AgentStatus> = { running: "busy", done: "done", error: "error" };

export function ToolCallCard({ call }: { call: ToolCallData }) {
  return (
    <div data-testid="toolcall" data-status={call.status}
         className="rounded-field border border-base-300 bg-base-200 px-3 py-2">
      <div className="flex items-center justify-between gap-2">
        <Badge tone="accent">{call.name}</Badge>
        <StatusDot status={AS[call.status]} />
      </div>
      {call.detail ? <div className="mt-1 font-mono text-xs text-base-content/70">{call.detail}</div> : null}
    </div>
  );
}
