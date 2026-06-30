import { useState } from "react";
import { useAtomValue } from "jotai";
import { useNavigate } from "@tanstack/react-router";
import { TeamList, TeamComposer, EmptyState } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { ALL_PROVIDERS } from "../lib/providers";
import type { Provider } from "../lib/providers";
import { useTeams, useCreateTeam, useConfiguredProviders } from "../data/queries";

export default function Teams() {
  const workspaceId = useAtomValue(activeWorkspaceIdAtom);
  const teams = useTeams(workspaceId);
  const create = useCreateTeam();
  const configured = useConfiguredProviders();
  const navigate = useNavigate();
  const [composing, setComposing] = useState(false);

  const providerOptions = ALL_PROVIDERS.filter((p) => (configured.data ?? []).includes(p.id as Provider));

  if (!workspaceId) return <EmptyState glyph="🗂" title="No workspace" description="Pick or create a workspace first." />;

  if (composing) {
    return (
      <TeamComposer
        providers={providerOptions}
        creating={create.isPending}
        onConfigureProviders={() => navigate({ to: "/settings" })}
        onCreate={(v) =>
          create.mutate(
            { workspace_id: workspaceId, name: v.name, problem: v.problem, auto: v.auto, members: v.members },
            { onSuccess: (t) => { setComposing(false); navigate({ to: "/teams/$teamId", params: { teamId: t.id } }); } },
          )
        }
      />
    );
  }

  return (
    <TeamList
      teams={(teams.data ?? []).map((t) => ({ id: t.id, name: t.name, problem: t.problem, memberCount: t.members.length }))}
      onOpen={(id) => navigate({ to: "/teams/$teamId", params: { teamId: id } })}
      onNew={() => setComposing(true)}
    />
  );
}
