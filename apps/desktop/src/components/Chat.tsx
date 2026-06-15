import { useState } from "react";
import { PROVIDERS, type Provider, sendPrompt } from "../lib/api";

type Msg = { role: "user" | "assistant" | "error"; text: string };

export function Chat({ providers }: { providers: Provider[] }) {
  const [provider, setProvider] = useState<Provider>(providers[0]);
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Msg[]>([]);
  const [busy, setBusy] = useState(false);

  async function send() {
    const text = input.trim();
    if (text === "" || busy) return;
    setInput("");
    setMessages((m) => [...m, { role: "user", text }]);
    setBusy(true);
    try {
      const reply = await sendPrompt(provider, text);
      setMessages((m) => [...m, { role: "assistant", text: reply }]);
    } catch (e) {
      setMessages((m) => [...m, { role: "error", text: String(e) }]);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-3 h-[70vh]">
      <select
        className="select select-bordered w-fit"
        value={provider}
        onChange={(e) => setProvider(e.target.value as Provider)}
      >
        {PROVIDERS.filter((p) => providers.includes(p.id)).map((p) => (
          <option key={p.id} value={p.id}>
            {p.label}
          </option>
        ))}
      </select>

      <div className="flex-1 overflow-y-auto rounded-box bg-base-100 p-4">
        {messages.map((m, i) => (
          <div
            key={i}
            className={`chat ${m.role === "user" ? "chat-end" : "chat-start"}`}
          >
            <div
              className={`chat-bubble ${
                m.role === "error" ? "chat-bubble-error" : ""
              }`}
            >
              {m.text}
            </div>
          </div>
        ))}
        {busy && <div className="chat chat-start"><div className="chat-bubble"><span className="loading loading-dots" /></div></div>}
      </div>

      <div className="flex gap-2">
        <input
          className="input input-bordered flex-1"
          placeholder="Ask something…"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && send()}
        />
        <button className="btn btn-primary" disabled={busy} onClick={send}>
          Send
        </button>
      </div>
    </div>
  );
}
