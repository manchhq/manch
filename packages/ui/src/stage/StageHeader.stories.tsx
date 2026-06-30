import type { Meta, StoryObj } from "@storybook/react";
import { StageHeader } from "./StageHeader";

const providers = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];
const meta: Meta<typeof StageHeader> = { title: "stage/StageHeader", component: StageHeader };
export default meta;
type Story = StoryObj<typeof StageHeader>;

export const Idle: Story = { args: { providers, activeProvider: "claude-code", onProviderChange: () => {}, status: "idle" } };
export const Busy: Story = { args: { providers, activeProvider: "anthropic", onProviderChange: () => {}, status: "busy" } };
