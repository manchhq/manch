import type { Meta, StoryObj } from "@storybook/react";
import { SettingsView } from "./SettingsView";

const meta: Meta<typeof SettingsView> = { title: "settings/SettingsView", component: SettingsView };
export default meta;
type Story = StoryObj<typeof SettingsView>;

export const Default: Story = {
  args: {
    providers: <div className="text-sm">Provider settings content</div>,
    theme: <div className="text-sm">Theme picker content</div>,
    workspaces: <div className="text-sm">Workspace settings content</div>,
  },
};
