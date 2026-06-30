import { useState } from "react";
import { useAtomValue } from "jotai";
import { useNavigate } from "@tanstack/react-router";
import { SearchBar, SearchResults, EmptyState } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useSearch } from "../data/queries";

export default function SearchPage() {
  const workspaceId = useAtomValue(activeWorkspaceIdAtom);
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState("");
  const navigate = useNavigate();
  const results = useSearch(workspaceId, query, []);
  if (!workspaceId) return <EmptyState glyph="🗂" title="No workspace" description="Pick or create a workspace first." />;
  const hits = results.data ?? [];
  return (
    <div className="space-y-4 p-6">
      <h1 className="text-xl font-semibold">Search</h1>
      <SearchBar value={draft} onChange={setDraft} onSubmit={() => setQuery(draft)} />
      {query && hits.length === 0
        ? <EmptyState glyph="🔍" title="No results" description={`Nothing matched "${query}".`} />
        : <SearchResults results={hits} onOpen={(kind, id) => { if (kind === "team") navigate({ to: "/teams/$teamId", params: { teamId: id } }); else navigate({ to: "/schedule" }); }} />}
    </div>
  );
}
