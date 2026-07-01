import type { JSX } from "react";
export interface ScheduleItemView {
  id: string;
  target: string;
  cadence: string;
  nextRun: string;
}

export interface ScheduleListProps {
  schedules: ScheduleItemView[];
}

export function ScheduleList({ schedules }: ScheduleListProps): JSX.Element {
  return (
    <ul className="space-y-2">
      {schedules.map((s) => (
        <li
          key={s.id}
          className="flex items-center justify-between rounded-box border border-base-300 px-3 py-2"
        >
          <div>
            <div className="font-medium text-base-content">{s.target}</div>
            <div className="text-xs text-base-content/60">next: {s.nextRun}</div>
          </div>
          <span className="badge badge-outline">{s.cadence}</span>
        </li>
      ))}
    </ul>
  );
}
