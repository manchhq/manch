import { useEffect, useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { listConfiguredProviders, type Provider } from "../lib/api";
import { Settings } from "../components/Settings";
import { Chat } from "../components/Chat";

export const Route = createFileRoute("/")({
  component: Home,
});

function Home() {
  const [providers, setProviders] = useState<Provider[] | null>(null);

  async function refresh() {
    setProviders(await listConfiguredProviders());
  }

  useEffect(() => {
    refresh();
  }, []);

  return (
    <main className="mx-auto flex max-w-3xl flex-col gap-6">
      <h1 className="text-2xl font-bold">Manch</h1>
      {providers === null ? (
        <span className="loading loading-spinner" />
      ) : providers.length === 0 ? (
        <Settings onSaved={refresh} />
      ) : (
        <>
          <Chat providers={providers} />
          <details className="collapse collapse-arrow bg-base-100">
            <summary className="collapse-title">Add another provider key</summary>
            <div className="collapse-content">
              <Settings onSaved={refresh} />
            </div>
          </details>
        </>
      )}
    </main>
  );
}
