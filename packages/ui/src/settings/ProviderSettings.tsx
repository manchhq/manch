import type { JSX } from "react";
import { useForm } from "@tanstack/react-form";

/** A selectable model for a BYOK provider (view-shape: camelCase, mapped from the DTO at the container boundary). */
export interface ModelOption {
  id: string;
  displayName: string | null;
}

export interface ProviderSettingsProps {
  all: { id: string; label: string }[];
  configured: string[];
  onSave: (provider: string, apiKey: string) => void;
  onRemove?: (provider: string) => void;
  saving?: boolean;
  /** Fetched models per BYOK provider id. Only present for configured BYOK providers — its absence for a given id is what keeps CLI providers dropdown-free. */
  models?: Record<string, ModelOption[]>;
  onModelChange?: (provider: string, model: string) => void;
}

export function ProviderSettings({
  all,
  configured,
  onSave,
  onRemove,
  saving,
  models,
  onModelChange,
}: ProviderSettingsProps): JSX.Element {
  const form = useForm({
    defaultValues: { provider: all[0]?.id ?? "", apiKey: "" },
    onSubmit: ({ value }) => onSave(value.provider, value.apiKey),
  });
  return (
    <section className="space-y-4">
      <h3 className="text-sm font-medium text-base-content">Providers</h3>
      <ul className="space-y-1">
        {all.map((p) => (
          <li key={p.id} className="flex items-center justify-between rounded-box border border-base-300 px-3 py-2">
            <span>{p.label}</span>
            {configured.includes(p.id) ? (
              <span className="flex items-center gap-2">
                <span className="badge badge-success badge-sm">configured</span>
                {models?.[p.id] && models[p.id].length > 0 && (
                  <label className="form-control">
                    <span className="sr-only">{p.label} model</span>
                    <select
                      aria-label={`${p.id} model`}
                      className="select select-bordered select-xs"
                      defaultValue={models[p.id][0].id}
                      onChange={(e) => onModelChange?.(p.id, e.target.value)}
                    >
                      {models[p.id].map((m) => (
                        <option key={m.id} value={m.id}>{m.displayName ?? m.id}</option>
                      ))}
                    </select>
                  </label>
                )}
                {onRemove && <button className="btn btn-ghost btn-xs" onClick={() => onRemove(p.id)}>Remove</button>}
              </span>
            ) : (
              <span className="badge badge-ghost badge-sm">not set</span>
            )}
          </li>
        ))}
      </ul>
      <form onSubmit={(e) => { e.preventDefault(); form.handleSubmit(); }} className="flex flex-wrap items-end gap-2">
        <form.Field name="provider">
          {(f) => (
            <label className="form-control">
              <span className="label-text">Provider</span>
              <select className="select select-bordered select-sm" value={f.state.value} onChange={(e) => f.handleChange(e.target.value)}>
                {all.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
              </select>
            </label>
          )}
        </form.Field>
        <form.Field name="apiKey">
          {(f) => (
            <label className="form-control">
              <span className="label-text">API key</span>
              <input aria-label="API key" type="password" className="input input-bordered input-sm" value={f.state.value} onChange={(e) => f.handleChange(e.target.value)} />
            </label>
          )}
        </form.Field>
        <button type="submit" className="btn btn-primary btn-sm" disabled={saving}>Save key</button>
      </form>
    </section>
  );
}
