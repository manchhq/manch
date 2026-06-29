import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { MessageData } from "../types";

export function Message({ message }: { message: MessageData }) {
  const isUser = message.role === "user";
  return (
    <div
      data-testid="message"
      data-role={message.role}
      className={`chat ${isUser ? "chat-end" : "chat-start"}`}
    >
      <div className={`chat-bubble ${isUser ? "chat-bubble-secondary" : "bg-base-100 text-base-content"}`}>
        <div className="prose prose-sm prose-invert max-w-none">
          <Markdown remarkPlugins={[remarkGfm]}>{message.text}</Markdown>
        </div>
      </div>
    </div>
  );
}
