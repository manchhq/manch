import type { ProviderOption } from "@manch/ui";

export const PROVIDERS = [
  { id: "anthropic", label: "Anthropic (Claude · BYOK)", kind: "byok" },
  { id: "gemini", label: "Google Gemini (BYOK)", kind: "byok" },
  { id: "openai", label: "OpenAI (Codex · BYOK)", kind: "byok" },
  { id: "claude-code", label: "Claude Code (ACP)", kind: "cli" },
  { id: "gemini-cli", label: "Gemini CLI (ACP)", kind: "cli" },
  { id: "codex", label: "Codex CLI (ACP)", kind: "cli" },
] as const;

export type Provider = (typeof PROVIDERS)[number]["id"];
export const ALL_PROVIDERS: ProviderOption[] = PROVIDERS.map((p) => ({ id: p.id, label: p.label }));

/** Providers with a fetchable/selectable model list (BYOK). CLI providers own their own model selection. */
export const BYOK_PROVIDERS: Provider[] = PROVIDERS.filter((p) => p.kind === "byok").map((p) => p.id);

export function isProvider(id: string): id is Provider {
  return (PROVIDERS as ReadonlyArray<{ id: string }>).some((p) => p.id === id);
}

export function isByokProvider(id: string): id is Provider {
  return (BYOK_PROVIDERS as readonly string[]).includes(id);
}
