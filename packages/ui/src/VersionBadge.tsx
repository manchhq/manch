export interface VersionBadgeProps {
  version: string | undefined;
  loading?: boolean;
  error?: string;
}

export function VersionBadge({ version, loading = false, error }: VersionBadgeProps) {
  if (error) return <span role="alert" className="badge badge-error">{error}</span>;
  if (loading || version === undefined) return <span role="status" className="badge">loading…</span>;
  return <span role="status" className="badge badge-success">v{version}</span>;
}
