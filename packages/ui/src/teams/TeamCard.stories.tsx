import type { Meta, StoryObj } from "@storybook/react";
import { TeamCard } from "./TeamCard";

const meta: Meta<typeof TeamCard> = {
  title: "teams/TeamCard",
  component: TeamCard,
};
export default meta;
type Story = StoryObj<typeof TeamCard>;

export const Default: Story = {
  args: {
    team: {
      id: "tm_1",
      name: "Discovery",
      problem: "find precedent",
      memberCount: 3,
    },
    onOpen: () => {},
  },
};
