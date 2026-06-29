import { describe, it, expect } from "vitest";
import { createStore } from "jotai";
import { conversationsAtom, activeIdAtom, activeConversationAtom, newConversation } from "./atoms";

describe("atoms", () => {
  it("newConversation produces an empty titled conversation", () => {
    const c = newConversation("Test");
    expect(c.title).toBe("Test");
    expect(c.messages).toEqual([]);
    expect(typeof c.id).toBe("string");
  });

  it("activeConversationAtom derives from conversations + activeId", () => {
    const store = createStore();
    const c = newConversation("Test");
    store.set(conversationsAtom, [c]);
    store.set(activeIdAtom, c.id);
    expect(store.get(activeConversationAtom)?.id).toBe(c.id);
  });
});
