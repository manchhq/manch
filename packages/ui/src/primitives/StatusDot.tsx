import type { AgentStatus } from "../types";

const TONE: Record<AgentStatus, string> = {
  idle: "bg-base-content/40",
  busy: "bg-warning animate-pulse",
  done: "bg-success",
  error: "bg-error",
};

export function StatusDot({ status, label }: { status: AgentStatus; label?: string }) {
  return (
    <span role="status" data-status={status} className="inline-flex items-center gap-2 text-sm text-base-content/70">
      <span className={`inline-block size-2 rounded-full ${TONE[status]}`} />
      {label}
    </span>
  );
}
