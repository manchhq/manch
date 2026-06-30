import { createFileRoute } from "@tanstack/react-router";
import { EmptyState } from "@manch/ui";

export const Route = createFileRoute("/settings")({ component: SettingsPage });

function SettingsPage() {
  return <EmptyState glyph="⚙️" title="Settings" description="Coming soon." />;
}
