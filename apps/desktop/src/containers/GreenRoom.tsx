import { useAtom, useSetAtom } from "jotai";
import { GreenRoomView } from "@manch/ui";
import {
  activeIdAtom,
  conversationsAtom,
  newConversation,
  settingsOpenAtom,
} from "../store/atoms";

export default function GreenRoom() {
  const [conversations, setConversations] = useAtom(conversationsAtom);
  const [activeId, setActiveId] = useAtom(activeIdAtom);
  const setSettingsOpen = useSetAtom(settingsOpenAtom);

  return (
    <GreenRoomView
      conversations={conversations.map((c) => ({ id: c.id, title: c.title }))}
      activeId={activeId}
      onSelect={(id) => setActiveId(id)}
      onNew={() => {
        const c = newConversation();
        setConversations((cs) => [c, ...cs]);
        setActiveId(c.id);
      }}
      onOpenSettings={() => setSettingsOpen(true)}
    />
  );
}
