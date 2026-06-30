import type { Meta, StoryObj } from "@storybook/react";
import { TeamList } from "./TeamList";

const meta: Meta<typeof TeamList> = {
  title: "teams/TeamList",
  component: TeamList,
};
export default meta;
type Story = StoryObj<typeof TeamList>;

export const Default: Story = {
  args: {
    teams: [
      {
        id: "tm_1",
        name: "Discovery",
        problem: "find precedent",
        memberCount: 3,
      },
      {
        id: "tm_2",
        name: "Drafting",
        problem: "draft contract",
        memberCount: 2,
      },
      {
        id: "tm_3",
        name: "Review",
        problem: "review document",
        memberCount: 1,
      },
    ],
    onOpen: () => {},
    onNew: () => {},
  },
};

export const Empty: Story = {
  args: {
    teams: [],
    onOpen: () => {},
    onNew: () => {},
  },
};
