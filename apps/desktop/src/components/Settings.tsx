import { useState } from "react";
import { PROVIDERS, type Provider, saveApiKey } from "../lib/api";

export function Settings({ onSaved }: { onSaved: () => void }) {
  const [provider, setProvider] = useState<Provider>("anthropic");
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await saveApiKey(provider, apiKey.trim());
      setApiKey("");
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="card bg-base-100 shadow">
      <div className="card-body gap-3">
        <h2 className="card-title">Add a provider key</h2>
        <select
          className="select select-bordered"
          value={provider}
          onChange={(e) => setProvider(e.target.value as Provider)}
        >
          {PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        <input
          type="password"
          className="input input-bordered"
          placeholder="API key"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
        />
        <button
          className="btn btn-primary"
          disabled={saving || apiKey.trim() === ""}
          onClick={save}
        >
          {saving ? "Saving…" : "Save key"}
        </button>
        {error && (
          <div role="alert" className="alert alert-error">
            <span>{error}</span>
          </div>
        )}
      </div>
    </div>
  );
}
