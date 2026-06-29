import type { Meta, StoryObj } from "@storybook/react";
import { PerformancePanel } from "./PerformancePanel";

const meta: Meta<typeof PerformancePanel> = { title: "stage/PerformancePanel", component: PerformancePanel };
export default meta;
type Story = StoryObj<typeof PerformancePanel>;

export const Idle: Story = { args: { status: "idle", toolCalls: [], files: [] } };
export const Performing: Story = {
  args: {
    status: "busy",
    toolCalls: [
      { id: "1", name: "read_file", status: "done", detail: "parser.rs" },
      { id: "2", name: "edit_file", status: "running", detail: "parser.rs" },
    ],
    files: ["parser.rs", "lexer.rs"],
  },
};
