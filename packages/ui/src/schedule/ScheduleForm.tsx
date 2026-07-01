import type { JSX } from "react";
import { useState } from "react";

export interface ScheduleFormValue {
  target: string;
  cadence: string;
  nextRun: string;
}

export interface ScheduleFormProps {
  onCreate: (value: ScheduleFormValue) => void;
  creating?: boolean;
}

export function ScheduleForm({
  onCreate,
  creating,
}: ScheduleFormProps): JSX.Element {
  const [target, setTarget] = useState("");
  const [cadence, setCadence] = useState("daily");
  const [nextRun, setNextRun] = useState("");

  return (
    <form
      className="flex flex-wrap items-end gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        onCreate({ target, cadence, nextRun });
      }}
    >
      <label className="form-control">
        <span className="label-text">Target</span>
        <input
          aria-label="target"
          className="input input-bordered input-sm"
          value={target}
          onChange={(e) => setTarget(e.target.value)}
        />
      </label>
      <label className="form-control">
        <span className="label-text">Cadence</span>
        <select
          aria-label="cadence"
          className="select select-bordered select-sm"
          value={cadence}
          onChange={(e) => setCadence(e.target.value)}
        >
          <option value="once">once</option>
          <option value="daily">daily</option>
          <option value="weekly">weekly</option>
        </select>
      </label>
      <label className="form-control">
        <span className="label-text">Next run</span>
        <input
          aria-label="next run"
          type="datetime-local"
          className="input input-bordered input-sm"
          value={nextRun}
          onChange={(e) => setNextRun(e.target.value)}
        />
      </label>
      <button
        type="submit"
        className="btn btn-primary btn-sm"
        disabled={creating}
      >
        Add schedule
      </button>
    </form>
  );
}
