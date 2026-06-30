import type { Meta, StoryObj } from "@storybook/react";
import { ProviderSettings } from "./ProviderSettings";
const meta: Meta<typeof ProviderSettings> = { title: "settings/ProviderSettings", component: ProviderSettings };
export default meta;
type Story = StoryObj<typeof ProviderSettings>;
const all = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];
export const None: Story = { args: { all, configured: [], onSave: () => {} } };
export const SomeConfigured: Story = { args: { all, configured: ["anthropic"], onSave: () => {}, onRemove: () => {} } };
