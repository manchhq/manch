import type { Meta, StoryObj } from "@storybook/react";
import { ProviderSettings } from "./ProviderSettings";
const meta: Meta<typeof ProviderSettings> = { title: "settings/ProviderSettings", component: ProviderSettings };
export default meta;
type Story = StoryObj<typeof ProviderSettings>;
const all = [{ id: "anthropic", label: "Anthropic" }, { id: "claude-code", label: "Claude Code" }];
export const None: Story = { args: { all, configured: [], onSave: () => {} } };
export const SomeConfigured: Story = { args: { all, configured: ["anthropic"], onSave: () => {}, onRemove: () => {} } };
export const WithModelDropdown: Story = {
  args: {
    all,
    configured: ["anthropic"],
    onSave: () => {},
    onRemove: () => {},
    models: {
      anthropic: [
        { id: "claude-opus-4-8", displayName: "Claude Opus 4.8" },
        { id: "claude-sonnet-5", displayName: "Claude Sonnet 5" },
      ],
    },
    onModelChange: () => {},
  },
};
