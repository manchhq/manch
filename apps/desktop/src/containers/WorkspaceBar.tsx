// WorkspaceBar.tsx
import { useEffect, useMemo } from "react";
import { useAtom } from "jotai";
import { WorkspaceSwitcher } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useWorkspaces, useCreateWorkspace } from "../data/queries";

export default function WorkspaceBar() {
  const [activeId, setActiveId] = useAtom(activeWorkspaceIdAtom);
  const workspaces = useWorkspaces();
  const create = useCreateWorkspace();

  // Memoize so the default-active effect doesn't re-run on every render (a
  // fresh `?? []` literal each render would keep re-firing the effect during
  // the loading window).
  const list = useMemo(() => workspaces.data ?? [], [workspaces.data]);
  // Pick a default when nothing is active AND self-correct when the persisted
  // active id points at a since-deleted workspace (the `!activeId` guard alone
  // never fires for a stale-but-present id, leaving the switcher stuck).
  useEffect(() => {
    if (list.length === 0) return;
    const resolves = activeId != null && list.some((w) => w.id === activeId);
    if (!resolves) setActiveId(list[0].id);
  }, [activeId, list, setActiveId]);

  return (
    <WorkspaceSwitcher
      workspaces={list.map((w) => ({ id: w.id, name: w.name }))}
      activeId={activeId}
      onSelect={setActiveId}
      onCreate={() =>
        create.mutate(
          { name: "New workspace", description: "" },
          { onSuccess: (w) => setActiveId(w.id) },
        )
      }
    />
  );
}
