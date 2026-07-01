import type { JSX } from "react";
import { useState } from "react";
import type { ProviderOption } from "../types";

export function SettingsForm({
  providers, onSave, saving = false, error = null,
}: {
  providers: ProviderOption[];
  onSave: (provider: string, apiKey: string) => void;
  saving?: boolean;
  error?: string | null;
}): JSX.Element {
  const [provider, setProvider] = useState(providers[0]?.id ?? "");
  const [apiKey, setApiKey] = useState("");
  return (
    <form
      className="card bg-base-100 shadow"
      onSubmit={(e) => { e.preventDefault(); onSave(provider, apiKey); }}
    >
      <div className="card-body gap-3">
        <h2 className="card-title text-base">Add a provider key</h2>
        <select className="select select-bordered" value={provider} onChange={(e) => setProvider(e.target.value)}>
          {providers.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
        </select>
        <label className="form-control">
          <span className="label-text">API key</span>
          <input
            className="input input-bordered" type="password" value={apiKey}
            onChange={(e) => setApiKey(e.target.value)} aria-label="API key"
          />
        </label>
        {error ? <div className="alert alert-error">{error}</div> : null}
        <button type="submit" className="btn btn-primary" disabled={saving || apiKey.length === 0}>
          {saving ? <span className="loading loading-spinner loading-sm" /> : "Save"}
        </button>
      </div>
    </form>
  );
}
