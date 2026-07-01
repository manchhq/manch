import type { JSX } from "react";
import type { ProviderOption, AgentStatus } from "../types";
import { StatusDot } from "../primitives/StatusDot";

export function StageHeader({
  providers, activeProvider, onProviderChange, status,
}: {
  providers: ProviderOption[];
  activeProvider: string;
  onProviderChange: (id: string) => void;
  status: AgentStatus;
}): JSX.Element {
  return (
    <header className="flex items-center justify-between border-b border-base-300 px-4 py-3 shadow-sm">
      <select
        className="select select-bordered select-sm"
        value={activeProvider}
        onChange={(e) => onProviderChange(e.target.value)}
      >
        {providers.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
      </select>
      <StatusDot status={status} label={status === "busy" ? "performing…" : status} />
    </header>
  );
}
