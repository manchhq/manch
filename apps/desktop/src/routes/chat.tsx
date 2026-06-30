import { createFileRoute } from "@tanstack/react-router";
import { useAtom } from "jotai";
import { Panel, IconRail } from "@manch/ui";
import GreenRoom from "../containers/GreenRoom";
import Stage from "../containers/Stage";
import Performance from "../containers/Performance";
import { leftCollapsedAtom, rightCollapsedAtom } from "../store/atoms";

export const Route = createFileRoute("/chat")({ component: Chat });

function Chat() {
  const [leftCollapsed, setLeft] = useAtom(leftCollapsedAtom);
  const [rightCollapsed, setRight] = useAtom(rightCollapsedAtom);
  return (
    <div className="grid h-full" style={{ gridTemplateColumns: `${leftCollapsed ? "2.5rem" : "16rem"} 1fr ${rightCollapsed ? "2.5rem" : "20rem"}` }}>
      {leftCollapsed ? (
        <div className="border-r border-base-300 bg-base-200">
          <IconRail items={[{ id: "expand", glyph: "»", label: "Expand", onClick: () => setLeft(false) }]} />
        </div>
      ) : (
        <div className="border-r border-base-300">
          <Panel title="Green Room" side="left" collapsed={false} onToggle={() => setLeft(true)}><GreenRoom /></Panel>
        </div>
      )}
      <main className="min-h-0 bg-base-100"><Stage /></main>
      {rightCollapsed ? (
        <div className="border-l border-base-300 bg-base-200">
          <IconRail items={[{ id: "expand", glyph: "«", label: "Expand", onClick: () => setRight(false) }]} />
        </div>
      ) : (
        <div className="border-l border-base-300">
          <Panel title="Performance" side="right" collapsed={false} onToggle={() => setRight(true)}><Performance /></Panel>
        </div>
      )}
    </div>
  );
}
