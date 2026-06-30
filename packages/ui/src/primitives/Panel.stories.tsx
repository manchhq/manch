import type { Meta, StoryObj } from "@storybook/react";
import { Panel } from "./Panel";

const meta: Meta<typeof Panel> = { title: "primitives/Panel", component: Panel };
export default meta;
type Story = StoryObj<typeof Panel>;

export const Expanded: Story = {
  args: { title: "Green Room", side: "left", collapsed: false, onToggle: () => {}, children: "panel body" },
};
export const Collapsed: Story = {
  args: { title: "Green Room", side: "left", collapsed: true, onToggle: () => {}, children: "panel body" },
};
