import type { JSX } from "react";
export interface TeamSummary {
  id: string;
  name: string;
  problem: string;
  memberCount: number;
}

export interface TeamCardProps {
  team: TeamSummary;
  onOpen: (id: string) => void;
}

export function TeamCard({ team, onOpen }: TeamCardProps): JSX.Element {
  return (
    <button
      onClick={() => onOpen(team.id)}
      aria-label={team.name}
      className="card w-full bg-base-200 p-4 text-left transition hover:bg-base-300"
    >
      <span className="text-base font-semibold text-base-content">{team.name}</span>
      {team.problem && <span className="mt-1 line-clamp-2 text-sm text-base-content/70">{team.problem}</span>}
      <span className="mt-2 text-xs text-base-content/50">{team.memberCount} member{team.memberCount === 1 ? "" : "s"}</span>
    </button>
  );
}
