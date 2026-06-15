export interface VersionBadgeProps {
  version: string | undefined;
  loading?: boolean;
  error?: string;
}

export function VersionBadge({ version, loading = false, error }: VersionBadgeProps) {
  if (error) return <span className="badge badge-error">{error}</span>;
  if (loading || version === undefined) return <span className="badge">loading…</span>;
  return <span className="badge badge-success">v{version}</span>;
}
