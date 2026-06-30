import { describe, it, expect } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { ReactNode } from "react";
import { useSend } from "./useSend";
import { conversationsAtom, activeIdAtom, agentStatusAtom, newConversation } from "../store/atoms";
import type { StageEngine } from "../engine/StageEngine";

const oneShot: StageEngine = {
  async *send() {
    yield { kind: "token", text: "hi" };
    yield { kind: "done" };
  },
};

const erroringEngine: StageEngine = {
  async *send() {
    yield { kind: "error", message: "boom" };
  },
};

const throwingEngine: StageEngine = {
  async *send() {
    throw new Error("boom");
  },
};

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: ReactNode }) => <Provider store={store}>{children}</Provider>;
}

describe("useSend", () => {
  it("appends user + agent messages and clears streaming", async () => {
    const store = createStore();
    const c = newConversation("t");
    store.set(conversationsAtom, [c]);
    store.set(activeIdAtom, c.id);

    const { result } = renderHook(() => useSend(oneShot), { wrapper: wrapper(store) });
    act(() => result.current.send("anthropic", "yo"));

    await waitFor(() => {
      const convo = store.get(conversationsAtom)[0];
      expect(convo.messages.map((m) => m.role)).toEqual(["user", "agent"]);
      expect(convo.messages[1].text).toBe("hi");
    });
  });

  it("on thrown engine: keeps only the user message and sets status error", async () => {
    const store = createStore();
    const c = newConversation("t");
    store.set(conversationsAtom, [c]);
    store.set(activeIdAtom, c.id);

    const { result } = renderHook(() => useSend(throwingEngine), { wrapper: wrapper(store) });
    act(() => result.current.send("anthropic", "yo"));

    await waitFor(() => {
      const convo = store.get(conversationsAtom)[0];
      expect(convo.messages.map((m) => m.role)).toEqual(["user"]);
      expect(store.get(agentStatusAtom)).toBe("error");
    });
  });

  it("on error: keeps only the user message and sets status error", async () => {
    const store = createStore();
    const c = newConversation("t");
    store.set(conversationsAtom, [c]);
    store.set(activeIdAtom, c.id);

    const { result } = renderHook(() => useSend(erroringEngine), { wrapper: wrapper(store) });
    act(() => result.current.send("anthropic", "yo"));

    await waitFor(() => {
      const convo = store.get(conversationsAtom)[0];
      expect(convo.messages.map((m) => m.role)).toEqual(["user"]);
      expect(store.get(agentStatusAtom)).toBe("error");
    });
  });
});
