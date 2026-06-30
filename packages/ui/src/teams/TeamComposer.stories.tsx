import type { Meta, StoryObj } from "@storybook/react";
import { TeamComposer } from "./TeamComposer";

const meta: Meta<typeof TeamComposer> = {
  title: "teams/TeamComposer",
  component: TeamComposer,
};
export default meta;
type Story = StoryObj<typeof TeamComposer>;

export const WithProviders: Story = {
  args: {
    providers: [
      { id: "anthropic", label: "Anthropic" },
      { id: "openai", label: "OpenAI" },
    ],
    onCreate: () => {},
    onConfigureProviders: () => {},
  },
};

export const NoProviders: Story = {
  args: {
    providers: [],
    onCreate: () => {},
    onConfigureProviders: () => {},
  },
};
