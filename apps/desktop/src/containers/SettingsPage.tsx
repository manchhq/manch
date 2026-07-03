import { useAtom } from "jotai";
import { SettingsView, ProviderSettings, ThemePicker, WorkspaceSettings } from "@manch/ui";
import type { ModelOption } from "@manch/ui";
import { themeAtom, THEMES } from "../store/atoms";
import { ALL_PROVIDERS, isByokProvider, type Provider } from "../lib/providers";
import {
  useConfiguredProviders,
  useSaveApiKey,
  useModelsForProviders,
  useSavedModelsForProviders,
  useSetModel,
  useWorkspaces,
  useRenameWorkspace,
  useDeleteWorkspace,
} from "../data/queries";

export default function SettingsPage() {
  const [theme, setTheme] = useAtom(themeAtom);
  const providers = useConfiguredProviders();
  const save = useSaveApiKey();
  const workspaces = useWorkspaces();
  const rename = useRenameWorkspace();
  const del = useDeleteWorkspace();

  const byokConfigured = (providers.data ?? []).filter(isByokProvider);
  const modelQueries = useModelsForProviders(byokConfigured);
  const savedModelQueries = useSavedModelsForProviders(byokConfigured);
  const setModel = useSetModel();

  const models: Record<string, ModelOption[]> = {};
  const selectedModels: Record<string, string> = {};
  byokConfigured.forEach((provider, i) => {
    const data = modelQueries[i]?.data;
    if (data) models[provider] = data.map((m) => ({ id: m.id, displayName: m.display_name }));
    const saved = savedModelQueries[i]?.data;
    if (typeof saved === "string") selectedModels[provider] = saved;
  });

  return (
    <SettingsView
      providers={
        <ProviderSettings
          all={ALL_PROVIDERS}
          configured={providers.data ?? []}
          saving={save.isPending}
          onSave={(provider, apiKey) => save.mutate({ provider: provider as Provider, apiKey })}
          models={models}
          selectedModels={selectedModels}
          onModelChange={(provider, model) => setModel.mutate({ provider: provider as Provider, model })}
        />
      }
      theme={<ThemePicker themes={THEMES} active={theme} onSelect={setTheme} />}
      workspaces={
        <WorkspaceSettings
          workspaces={(workspaces.data ?? []).map((w) => ({ id: w.id, name: w.name }))}
          onRename={(id, name) => rename.mutate({ id, name })}
          onDelete={(id) => del.mutate(id)}
        />
      }
    />
  );
}
