import { useAtom } from "jotai";
import { useNavigate } from "@tanstack/react-router";
import { GreenRoomView } from "@manch/ui";
import { activeIdAtom, conversationsAtom, newConversation } from "../store/atoms";

export default function GreenRoom() {
  const [conversations, setConversations] = useAtom(conversationsAtom);
  const [activeId, setActiveId] = useAtom(activeIdAtom);
  const navigate = useNavigate();

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
      onOpenSettings={() => navigate({ to: "/settings" })}
    />
  );
}
