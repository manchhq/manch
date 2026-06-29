import type { Meta, StoryObj } from "@storybook/react";
import { Transcript } from "./Transcript";

const meta: Meta<typeof Transcript> = { title: "stage/Transcript", component: Transcript };
export default meta;
type Story = StoryObj<typeof Transcript>;

export const Empty: Story = { args: { messages: [] } };
export const Conversation: Story = {
  args: { messages: [
    { id: "1", role: "user", text: "refactor the parser" },
    { id: "2", role: "agent", text: "Reading `parser.rs` first." },
  ] },
};
export const Streaming: Story = {
  args: { messages: [{ id: "1", role: "user", text: "hi" }], isStreaming: true, streamingText: "I'll start by" },
};
