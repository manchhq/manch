import { createRootRoute, Outlet, useRouterState, useNavigate } from "@tanstack/react-router";
import { Provider as JotaiProvider, useAtomValue } from "jotai";
import { useEffect } from "react";
import { NavRail } from "@manch/ui";
import { themeAtom } from "../store/atoms";
import WorkspaceBar from "../containers/WorkspaceBar";
import "../styles.css";

export const Route = createRootRoute({ component: RootShell });

const NAV = [
  { id: "/chat", label: "Chat", glyph: "💬" },
  { id: "/teams", label: "Teams", glyph: "👥" },
  { id: "/schedule", label: "Schedule", glyph: "📅" },
  { id: "/search", label: "Search", glyph: "🔍" },
  { id: "/settings", label: "Settings", glyph: "⚙️" },
];

function Shell() {
  const theme = useAtomValue(themeAtom);
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const activeId = NAV.find((n) => pathname.startsWith(n.id))?.id ?? "/chat";

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="flex h-screen flex-col bg-base-100 text-base-content">
      <header className="flex items-center gap-3 border-b border-base-300 px-3 py-2">
        <WorkspaceBar />
        <span className="font-semibold tracking-wide">Manch</span>
      </header>
      <div className="grid min-h-0 flex-1" style={{ gridTemplateColumns: "3.5rem 1fr" }}>
        <NavRail items={NAV} activeId={activeId} onSelect={(id) => navigate({ to: id })} />
        <main className="min-h-0 overflow-hidden"><Outlet /></main>
      </div>
    </div>
  );
}

function RootShell() {
  return (
    <JotaiProvider>
      <Shell />
    </JotaiProvider>
  );
}
