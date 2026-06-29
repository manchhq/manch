import type { Meta, StoryObj } from "@storybook/react";
import { IconRail } from "./IconRail";

const meta: Meta<typeof IconRail> = { title: "primitives/IconRail", component: IconRail };
export default meta;
type Story = StoryObj<typeof IconRail>;

export const Default: Story = {
  args: {
    items: [
      { id: "new", glyph: "+", label: "New conversation", onClick: () => {} },
      { id: "keys", glyph: "⚙", label: "Keys", onClick: () => {} },
    ],
  },
};
