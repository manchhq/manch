import { createFileRoute } from "@tanstack/react-router";
import { EmptyState } from "@manch/ui";

export const Route = createFileRoute("/teams")({ component: Teams });

function Teams() {
  return <EmptyState glyph="👥" title="Teams" description="Coming soon." />;
}
