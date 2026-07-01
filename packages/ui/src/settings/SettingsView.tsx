import type { JSX } from "react";
import type { ReactNode } from "react";

export interface SettingsViewProps {
  providers: ReactNode;
  theme: ReactNode;
  workspaces: ReactNode;
}

export function SettingsView({ providers, theme, workspaces }: SettingsViewProps): JSX.Element {
  return (
    <div className="mx-auto max-w-2xl space-y-8 overflow-y-auto p-6">
      <h1 className="text-xl font-semibold">Settings</h1>
      <div className="rounded-box border border-base-300 p-4">{providers}</div>
      <div className="rounded-box border border-base-300 p-4">{theme}</div>
      <div className="rounded-box border border-base-300 p-4">{workspaces}</div>
    </div>
  );
}
