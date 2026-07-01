import { useParams } from "@tanstack/react-router";
import { useAtomValue } from "jotai";
import { TeamDetail, EmptyState } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useTeam, useAssignTeamTask } from "../data/queries";

export default function TeamDetailPage() {
  const { teamId } = useParams({ from: "/teams/$teamId" });
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom);
  const team = useTeam(teamId);
  const assign = useAssignTeamTask();

  if (team.isLoading) return <EmptyState glyph="⏳" title="Loading…" />;
  // Scope to the active workspace: a direct /teams/$teamId URL for another
  // workspace's team must not render here.
  if (!team.data || team.data.workspace_id !== activeWorkspaceId)
    return <EmptyState glyph="❓" title="Team not found" />;

  const run = assign.data
    ? { task: assign.data.task, result: assign.data.result, steps: assign.data.steps.map((s) => ({ memberRole: s.member_role, detail: s.detail, status: s.status as "running" | "done" | "error" })) }
    : null;

  return (
    <TeamDetail
      name={team.data.name}
      problem={team.data.problem}
      members={team.data.members.map((m) => ({ role: m.role, provider: m.provider }))}
      capabilities={team.data.capabilities}
      run={run}
      assigning={assign.isPending}
      onAssign={(task) => assign.mutate({ teamId, task })}
    />
  );
}
