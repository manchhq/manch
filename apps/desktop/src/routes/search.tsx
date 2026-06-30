import { createFileRoute } from "@tanstack/react-router";
import { EmptyState } from "@manch/ui";

export const Route = createFileRoute("/search")({ component: Search });

function Search() {
  return <EmptyState glyph="🔍" title="Search" description="Coming soon." />;
}
