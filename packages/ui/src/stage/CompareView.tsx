import type { JSX } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

export interface CompareReport {
  provider: string;
  text: string;
}

export interface CompareViewProps {
  reports: CompareReport[];
  summary: string;
}

export function CompareView({ reports, summary }: CompareViewProps): JSX.Element {
  return (
    <div className="space-y-4 p-4">
      <div
        className="grid gap-3"
        style={{
          gridTemplateColumns: `repeat(${Math.max(reports.length, 1)}, minmax(0, 1fr))`,
        }}
      >
        {reports.map((r) => (
          <article
            key={r.provider}
            className="rounded-box border border-base-300 bg-base-100 p-3"
          >
            <div className="mb-2 badge badge-primary badge-sm">{r.provider}</div>
            <div className="prose prose-sm max-w-none">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {r.text}
              </ReactMarkdown>
            </div>
          </article>
        ))}
      </div>
      <div className="rounded-box border border-primary/40 bg-base-200 p-3">
        <div className="mb-1 text-sm font-medium text-primary">
          Cross-verification
        </div>
        <div className="prose prose-sm max-w-none">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {summary}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  );
}
