// WorkspaceBar.tsx
import { useEffect } from "react";
import { useAtom } from "jotai";
import { WorkspaceSwitcher } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useWorkspaces, useCreateWorkspace } from "../data/queries";

export default function WorkspaceBar() {
  const [activeId, setActiveId] = useAtom(activeWorkspaceIdAtom);
  const workspaces = useWorkspaces();
  const create = useCreateWorkspace();

  const list = workspaces.data ?? [];
  useEffect(() => {
    if (!activeId && list.length > 0) setActiveId(list[0].id);
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
