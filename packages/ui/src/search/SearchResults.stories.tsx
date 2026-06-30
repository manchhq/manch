import { Meta, StoryObj } from "@storybook/react";
import { SearchResults } from "./SearchResults";

const meta: Meta<typeof SearchResults> = {
  component: SearchResults,
  title: "SearchResults",
};

export default meta;
type Story = StoryObj<typeof SearchResults>;

export const Default: Story = {
  args: {
    results: [
      { kind: "team", id: "tm_1", title: "Discovery", snippet: "find precedent" },
      { kind: "schedule", id: "sch_2", title: "Q4 Planning", snippet: "quarterly schedule" },
    ],
    onOpen: () => {},
  },
};

export const Empty: Story = {
  args: {
    results: [],
    onOpen: () => {},
  },
};
