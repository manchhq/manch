import { invoke } from "@tauri-apps/api/core";

export type Provider = "anthropic" | "claude-code";

export const PROVIDERS: { id: Provider; label: string }[] = [
  { id: "anthropic", label: "Anthropic (Claude · BYOK)" },
  { id: "claude-code", label: "Claude Code (ACP)" },
];

/** Tauri maps camelCase JS args to snake_case Rust params (apiKey -> api_key). */
export const saveApiKey = (provider: Provider, apiKey: string): Promise<void> =>
  invoke("save_api_key", { provider, apiKey });

export const listConfiguredProviders = (): Promise<Provider[]> =>
  invoke("list_configured_providers");

export const sendPrompt = (provider: Provider, text: string): Promise<string> =>
  invoke("send_prompt", { provider, text });
