import { useAtom } from "jotai";
import { SettingsView, ProviderSettings, ThemePicker, WorkspaceSettings } from "@manch/ui";
import { themeAtom, THEMES } from "../store/atoms";
import { ALL_PROVIDERS } from "../lib/providers";
import {
  useConfiguredProviders,
  useSaveApiKey,
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

  return (
    <SettingsView
      providers={
        <ProviderSettings
          all={ALL_PROVIDERS}
          configured={providers.data ?? []}
          saving={save.isPending}
          onSave={(provider, apiKey) => save.mutate({ provider: provider as never, apiKey })}
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
