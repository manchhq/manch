import { useState } from "react";

export interface ComposerMember {
  role: string;
  provider: string;
}
export interface TeamComposerValue {
  name: string;
  problem: string;
  auto: boolean;
  members: ComposerMember[];
}
export interface TeamComposerProps {
  providers: { id: string; label: string }[];
  onCreate: (value: TeamComposerValue) => void;
  onConfigureProviders?: () => void;
  creating?: boolean;
}

export function TeamComposer({ providers, onCreate, onConfigureProviders, creating }: TeamComposerProps) {
  const [auto, setAuto] = useState(true);
  const [name, setName] = useState("");
  const [problem, setProblem] = useState("");
  const [members, setMembers] = useState<ComposerMember[]>([]);
  const noProviders = providers.length === 0;

  const submit = () => onCreate({ name: auto ? name || "New team" : name, problem, auto, members: auto ? [] : members });

  return (
    <form className="space-y-4 p-6" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <h1 className="text-xl font-semibold">New team</h1>

      <div role="tablist" className="tabs tabs-boxed w-fit">
        <button type="button" role="tab" className={`tab ${auto ? "tab-active" : ""}`} onClick={() => setAuto(true)}>Auto-compose</button>
        <button type="button" role="tab" className={`tab ${!auto ? "tab-active" : ""}`} onClick={() => setAuto(false)}>Manual</button>
      </div>

      {noProviders && (
        <div className="alert alert-warning">
          <span>No AI providers configured.</span>
          <button type="button" className="btn btn-sm" onClick={() => onConfigureProviders?.()}>Add an AI provider in Settings</button>
        </div>
      )}

      {auto ? (
        <label className="form-control">
          <span className="label-text">Problem</span>
          <textarea aria-label="problem" className="textarea textarea-bordered" rows={3}
            value={problem} onChange={(e) => setProblem(e.target.value)}
            placeholder="Describe the problem; an AI will compose the team." />
        </label>
      ) : (
        <div className="space-y-3">
          <label className="form-control">
            <span className="label-text">Team name</span>
            <input className="input input-bordered input-sm" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <ul className="space-y-2">
            {members.map((m, i) => (
              <li key={i} className="flex gap-2">
                <input aria-label={`role ${i}`} className="input input-bordered input-sm flex-1" placeholder="Role"
                  value={m.role} onChange={(e) => setMembers((ms) => ms.map((x, j) => j === i ? { ...x, role: e.target.value } : x))} />
                <select aria-label={`provider ${i}`} className="select select-bordered select-sm" disabled={noProviders}
                  value={m.provider} onChange={(e) => setMembers((ms) => ms.map((x, j) => j === i ? { ...x, provider: e.target.value } : x))}>
                  {providers.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
                </select>
                <button type="button" className="btn btn-ghost btn-sm" onClick={() => setMembers((ms) => ms.filter((_, j) => j !== i))}>✕</button>
              </li>
            ))}
          </ul>
          <button type="button" className="btn btn-sm" disabled={noProviders}
            onClick={() => setMembers((ms) => [...ms, { role: "", provider: providers[0]?.id ?? "" }])}>＋ Add member</button>
        </div>
      )}

      <button type="submit" className="btn btn-primary btn-sm" disabled={creating}>Create team</button>
    </form>
  );
}
