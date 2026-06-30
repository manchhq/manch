import type { Meta, StoryObj } from "@storybook/react";
import { EmptyState } from "./EmptyState";

const meta: Meta<typeof EmptyState> = { title: "primitives/EmptyState", component: EmptyState };
export default meta;
type Story = StoryObj<typeof EmptyState>;

export const Bare: Story = { args: { glyph: "🎭", title: "Nothing here yet" } };
export const WithAction: Story = {
  args: { glyph: "👥", title: "No teams", description: "Spin up a team to get started.", action: { label: "New team", onClick: () => {} } },
};
