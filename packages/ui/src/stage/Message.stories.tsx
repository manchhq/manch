import type { Meta, StoryObj } from "@storybook/react";
import { Message } from "./Message";

const meta: Meta<typeof Message> = { title: "stage/Message", component: Message };
export default meta;
type Story = StoryObj<typeof Message>;

export const Agent: Story = {
  args: { message: { id: "a", role: "agent", text: "I'll start by reading `parser.rs`.\n\n- step one\n- step two" } },
};
export const User: Story = { args: { message: { id: "u", role: "user", text: "refactor the parser" } } };
