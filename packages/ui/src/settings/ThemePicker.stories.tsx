import type { Meta, StoryObj } from "@storybook/react";
import { ThemePicker } from "./ThemePicker";

const meta: Meta<typeof ThemePicker> = { title: "settings/ThemePicker", component: ThemePicker };
export default meta;
type Story = StoryObj<typeof ThemePicker>;

export const Default: Story = {
  args: { themes: ["dark", "light", "dracula", "nord", "cupcake"], active: "dark", onSelect: () => {} },
};
