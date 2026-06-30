import { TeamCard, type TeamSummary } from "./TeamCard";

export interface TeamListProps {
  teams: TeamSummary[];
  onOpen: (id: string) => void;
  onNew: () => void;
}

export function TeamList({ teams, onOpen, onNew }: TeamListProps) {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Teams</h1>
        <button className="btn btn-primary btn-sm" onClick={onNew}>＋ New team</button>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {teams.map((t) => <TeamCard key={t.id} team={t} onOpen={onOpen} />)}
      </div>
    </div>
  );
}
