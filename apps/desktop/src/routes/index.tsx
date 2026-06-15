import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@connectrpc/connect-query";
import { versionQuery, createManchTransport } from "@manch/api";
import { VersionBadge } from "@manch/ui";

export const Route = createFileRoute("/")({
  component: Home,
});

const transport = createManchTransport();

function Home() {
  const { data, isLoading, error } = useQuery(versionQuery.getVersion, {}, { transport });

  return (
    <main className="flex flex-col gap-4">
      <h1 className="text-2xl font-bold">Manch</h1>
      <div className="flex items-center gap-2">
        <span>server version:</span>
        <VersionBadge
          version={data?.version}
          loading={isLoading}
          error={error ? "unreachable" : undefined}
        />
      </div>
    </main>
  );
}
