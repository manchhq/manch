import type { Meta, StoryObj } from "@storybook/react";
import { GreenRoomView } from "./GreenRoomView";

const conversations = [{ id: "a", title: "Refactor auth" }, { id: "b", title: "Draft README" }];
const meta: Meta<typeof GreenRoomView> = { title: "stage/GreenRoomView", component: GreenRoomView };
export default meta;
type Story = StoryObj<typeof GreenRoomView>;

export const Populated: Story = {
  args: { conversations, activeId: "a", onSelect: () => {}, onNew: () => {}, onOpenSettings: () => {} },
};
export const Empty: Story = {
  args: { conversations: [], activeId: null, onSelect: () => {}, onNew: () => {}, onOpenSettings: () => {} },
};
