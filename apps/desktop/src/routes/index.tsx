import { createFileRoute } from "@tanstack/react-router";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Panel, IconRail } from "@manch/ui";
import GreenRoom from "../containers/GreenRoom";
import Stage from "../containers/Stage";
import Performance from "../containers/Performance";
import Settings from "../containers/Settings";
import {
  leftCollapsedAtom, rightCollapsedAtom, settingsOpenAtom, conversationsAtom,
  activeIdAtom, newConversation,
} from "../store/atoms";

export const Route = createFileRoute("/")({ component: Home });

export function Home() {
  const [leftCollapsed, setLeft] = useAtom(leftCollapsedAtom);
  const [rightCollapsed, setRight] = useAtom(rightCollapsedAtom);
  const [settingsOpen, setSettingsOpen] = useAtom(settingsOpenAtom);
  const conversations = useAtomValue(conversationsAtom);
  const setConversations = useSetAtom(conversationsAtom);
  const setActiveId = useSetAtom(activeIdAtom);

  const startConversation = () => {
    const c = newConversation();
    setConversations((cs) => [c, ...cs]);
    setActiveId(c.id);
    setSettingsOpen(false);
  };

  if (settingsOpen || conversations.length === 0) {
    return (
      <div className="flex h-full flex-col">
        <header className="flex items-center justify-between border-b border-base-300 px-4 py-2">
          <h1 className="text-lg font-semibold">Manch</h1>
          <div className="flex gap-2">
            <button className="btn btn-primary btn-sm" onClick={startConversation}>New conversation</button>
            {conversations.length > 0 && (
              <button className="btn btn-ghost btn-sm" onClick={() => setSettingsOpen(false)}>Back to stage</button>
            )}
          </div>
        </header>
        <main className="min-h-0 flex-1 overflow-y-auto"><Settings /></main>
      </div>
    );
  }

  return (
    <div className="grid h-full" style={{ gridTemplateColumns: `${leftCollapsed ? "2.5rem" : "16rem"} 1fr ${rightCollapsed ? "2.5rem" : "20rem"}` }}>
      {leftCollapsed ? (
        <div className="border-r border-base-300 bg-base-200">
          <IconRail items={[{ id: "expand", glyph: "»", label: "Expand", onClick: () => setLeft(false) }]} />
        </div>
      ) : (
        <div className="border-r border-base-300">
          <Panel title="Green Room" side="left" collapsed={false} onToggle={() => setLeft(true)}>
            <GreenRoom />
          </Panel>
        </div>
      )}

      <main className="min-h-0 bg-base-100"><Stage /></main>

      {rightCollapsed ? (
        <div className="border-l border-base-300 bg-base-200">
          <IconRail items={[{ id: "expand", glyph: "«", label: "Expand", onClick: () => setRight(false) }]} />
        </div>
      ) : (
        <div className="border-l border-base-300">
          <Panel title="Performance" side="right" collapsed={false} onToggle={() => setRight(true)}>
            <Performance />
          </Panel>
        </div>
      )}
    </div>
  );
}
