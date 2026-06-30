import type { ProviderOption } from "@manch/ui";

export const PROVIDERS = [
  { id: "anthropic", label: "Anthropic (Claude · BYOK)" },
  { id: "claude-code", label: "Claude Code (ACP)" },
] as const;

export type Provider = (typeof PROVIDERS)[number]["id"];
export const ALL_PROVIDERS: ProviderOption[] = PROVIDERS.map((p) => ({ id: p.id, label: p.label }));

export function isProvider(id: string): id is Provider {
  return (PROVIDERS as ReadonlyArray<{ id: string }>).some((p) => p.id === id);
}
