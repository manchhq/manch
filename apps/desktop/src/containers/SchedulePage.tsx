import { useAtomValue } from "jotai";
import { ScheduleList, ScheduleForm, EmptyState } from "@manch/ui";
import { activeWorkspaceIdAtom } from "../store/atoms";
import { useSchedules, useCreateSchedule } from "../data/queries";

export default function SchedulePage() {
  const workspaceId = useAtomValue(activeWorkspaceIdAtom);
  const schedules = useSchedules(workspaceId);
  const create = useCreateSchedule();
  if (!workspaceId) return <EmptyState glyph="🗂" title="No workspace" description="Pick or create a workspace first." />;
  const items = (schedules.data ?? []).map((s) => ({ id: s.id, target: s.target, cadence: s.cadence, nextRun: s.next_run }));
  return (
    <div className="space-y-6 p-6">
      <h1 className="text-xl font-semibold">Schedule</h1>
      <ScheduleForm creating={create.isPending}
        onCreate={(v) => create.mutate({ workspace_id: workspaceId, target: v.target, cadence: v.cadence, next_run: v.nextRun })} />
      {items.length === 0
        ? <EmptyState glyph="📅" title="No schedules yet" description="Add one above." />
        : <ScheduleList schedules={items} />}
    </div>
  );
}
