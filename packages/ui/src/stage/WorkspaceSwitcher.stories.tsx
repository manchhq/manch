import type { Meta, StoryObj } from "@storybook/react";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

const meta: Meta<typeof WorkspaceSwitcher> = { title: "stage/WorkspaceSwitcher", component: WorkspaceSwitcher };
export default meta;
type Story = StoryObj<typeof WorkspaceSwitcher>;

export const Default: Story = {
  args: {
    workspaces: [{ id: "w1", name: "Legal research" }, { id: "w2", name: "Health" }],
    activeId: "w1", onSelect: () => {}, onCreate: () => {},
  },
};
export const Empty: Story = { args: { workspaces: [], activeId: null, onSelect: () => {}, onCreate: () => {} } };
