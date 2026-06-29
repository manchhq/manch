import type { Meta, StoryObj } from "@storybook/react";
import { StatusDot } from "./StatusDot";

const meta: Meta<typeof StatusDot> = { title: "primitives/StatusDot", component: StatusDot };
export default meta;
type Story = StoryObj<typeof StatusDot>;

export const Idle: Story = { args: { status: "idle", label: "idle" } };
export const Busy: Story = { args: { status: "busy", label: "performing…" } };
export const Done: Story = { args: { status: "done", label: "done" } };
export const Error: Story = { args: { status: "error", label: "failed" } };
