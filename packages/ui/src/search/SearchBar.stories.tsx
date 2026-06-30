import { Meta, StoryObj } from "@storybook/react";
import { SearchBar } from "./SearchBar";

const meta: Meta<typeof SearchBar> = {
  component: SearchBar,
  title: "SearchBar",
};

export default meta;
type Story = StoryObj<typeof SearchBar>;

export const Default: Story = {
  args: {
    value: "",
    onChange: () => {},
    onSubmit: () => {},
  },
};

export const WithValue: Story = {
  args: {
    value: "precedent",
    onChange: () => {},
    onSubmit: () => {},
  },
};
