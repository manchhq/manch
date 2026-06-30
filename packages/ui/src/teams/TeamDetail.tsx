import type { JSX } from "react";
import { useState } from "react";
import type { AgentStatus } from "../types";
import { StatusDot } from "../primitives/StatusDot";

export interface DetailMember { role: string; provider: string }
export interface DetailRunStep { memberRole: string; detail: string; status: "running" | "done" | "error" }
export interface TeamRunView { task: string; steps: DetailRunStep[]; result: string }
export interface TeamDetailProps {
  name: string;
  problem: string;
  members: DetailMember[];
  capabilities: string[];
  run?: TeamRunView | null;
  onAssign: (task: string) => void;
  assigning?: boolean;
}

const statusMap: Record<"running" | "done" | "error", AgentStatus> = {
  running: "busy",
  done: "done",
  error: "error",
};

export function TeamDetail({ name, problem, members, capabilities, run, onAssign, assigning }: TeamDetailProps): JSX.Element {
  const [task, setTask] = useState("");
  return (
    <div className="space-y-6 overflow-y-auto p-6">
      <header>
        <h1 className="text-xl font-semibold">{name}</h1>
        {problem && <p className="text-sm text-base-content/70">{problem}</p>}
      </header>

      <section>
        <h2 className="mb-2 text-sm font-medium">Members</h2>
        <ul className="space-y-1">
          {members.map((m, i) => (
            <li key={i} className="flex items-center justify-between rounded-box border border-base-300 px-3 py-2">
              <span className="font-medium">{m.role}</span>
              <span className="badge badge-ghost badge-sm">{m.provider}</span>
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2 className="mb-2 text-sm font-medium">Capabilities</h2>
        <div className="flex flex-wrap gap-2">
          {capabilities.map((c) => <span key={c} className="badge badge-outline">{c}</span>)}
        </div>
      </section>

      <section className="space-y-2">
        <h2 className="text-sm font-medium">Assign a task</h2>
        <div className="flex gap-2">
          <input aria-label="task" className="input input-bordered input-sm flex-1" value={task} onChange={(e) => setTask(e.target.value)} />
          <button className="btn btn-primary btn-sm" disabled={assigning || !task} onClick={() => onAssign(task)}>Assign</button>
        </div>
      </section>

      {run && (
        <section className="space-y-2">
          <h2 className="text-sm font-medium">Run</h2>
          <ol className="space-y-1">
            {run.steps.map((s, i) => (
              <li key={i} className="flex items-center gap-2 rounded-box border border-base-300 px-3 py-2 text-sm">
                <StatusDot status={statusMap[s.status]} live={false} />
                <span className="font-medium">{s.memberRole}</span>
                <span className="text-base-content/70">{s.detail}</span>
              </li>
            ))}
          </ol>
          <div className="rounded-box bg-base-200 p-3 text-sm">{run.result}</div>
        </section>
      )}
    </div>
  );
}
