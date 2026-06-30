import type { Meta, StoryObj } from "@storybook/react";
import { Badge } from "./Badge";

const meta: Meta<typeof Badge> = { title: "primitives/Badge", component: Badge };
export default meta;
type Story = StoryObj<typeof Badge>;

export const Neutral: Story = { args: { children: "neutral", tone: "neutral" } };
export const Accent: Story = { args: { children: "tool", tone: "accent" } };
export const Error: Story = { args: { children: "error", tone: "error" } };
