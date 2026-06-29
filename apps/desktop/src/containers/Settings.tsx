import { SettingsForm } from "@manch/ui";
import { ALL_PROVIDERS } from "../lib/providers";
import { useSaveApiKey } from "../data/queries";

export default function Settings() {
  const save = useSaveApiKey();
  return (
    <div className="mx-auto max-w-md p-6">
      <SettingsForm
        providers={ALL_PROVIDERS}
        saving={save.isPending}
        error={save.isError ? String(save.error) : null}
        onSave={(provider, apiKey) => save.mutate({ provider, apiKey })}
      />
    </div>
  );
}
