import type { Meta, StoryObj } from "@storybook/react";
import { SettingsForm } from "./SettingsForm";

const providers = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];
const meta: Meta<typeof SettingsForm> = { title: "stage/SettingsForm", component: SettingsForm };
export default meta;
type Story = StoryObj<typeof SettingsForm>;

export const Default: Story = { args: { providers, onSave: () => {} } };
export const Error: Story = { args: { providers, onSave: () => {}, error: "anthropic: invalid x-api-key" } };
