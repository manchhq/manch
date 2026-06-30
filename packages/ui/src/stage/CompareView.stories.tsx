import type { Meta, StoryObj } from "@storybook/react";
import { CompareView } from "./CompareView";

const meta: Meta<typeof CompareView> = {
  title: "stage/CompareView",
  component: CompareView,
};
export default meta;
type Story = StoryObj<typeof CompareView>;

export const TwoProviders: Story = {
  args: {
    reports: [
      {
        provider: "anthropic",
        text: "The issue appears to be a race condition in the async state update. The handler should use a ref to track the current state.",
      },
      {
        provider: "claude-code",
        text: "I agree. A potential solution is to wrap the state update in a useCallback with proper dependency tracking.",
      },
    ],
    summary:
      "Both analyses agree on the root cause: async state synchronization. Recommended fix: use useCallback + ref pattern to ensure state consistency.",
  },
};
