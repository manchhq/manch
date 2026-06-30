import { createFileRoute } from "@tanstack/react-router";
import { EmptyState } from "@manch/ui";

export const Route = createFileRoute("/schedule")({ component: Schedule });

function Schedule() {
  return <EmptyState glyph="📅" title="Schedule" description="Coming soon." />;
}
