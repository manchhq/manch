import type { ReactNode } from "react";

const TONE = {
  neutral: "badge-neutral",
  primary: "badge-primary",
  accent: "badge-accent",
  error: "badge-error",
} as const;

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: keyof typeof TONE }) {
  return <span className={`badge ${TONE[tone]}`}>{children}</span>;
}
