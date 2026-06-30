import type { Meta, StoryObj } from "@storybook/react";
import { NavRail } from "./NavRail";

const meta: Meta<typeof NavRail> = { title: "primitives/NavRail", component: NavRail };
export default meta;
type Story = StoryObj<typeof NavRail>;

const items = [
  { id: "chat", label: "Chat", glyph: "💬" },
  { id: "teams", label: "Teams", glyph: "👥" },
  { id: "schedule", label: "Schedule", glyph: "📅" },
  { id: "search", label: "Search", glyph: "🔍" },
  { id: "settings", label: "Settings", glyph: "⚙️" },
];

export const Default: Story = { args: { items, activeId: "chat", onSelect: () => {} } };
