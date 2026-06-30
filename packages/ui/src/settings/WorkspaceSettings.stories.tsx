import type { Meta, StoryObj } from "@storybook/react";
import { WorkspaceSettings } from "./WorkspaceSettings";

const meta: Meta<typeof WorkspaceSettings> = {
  title: "settings/WorkspaceSettings",
  component: WorkspaceSettings,
};

export default meta;
type Story = StoryObj<typeof WorkspaceSettings>;

export const Default: Story = {
  args: {
    workspaces: [
      { id: "w1", name: "Main" },
      { id: "w2", name: "Dev" },
    ],
    onRename: () => {},
    onDelete: () => {},
  },
};
