import type { Meta, StoryObj } from "@storybook/react";
import { ScheduleForm } from "./ScheduleForm";

const meta: Meta<typeof ScheduleForm> = {
  title: "Components/Schedule/ScheduleForm",
  component: ScheduleForm,
  args: { onCreate: () => {}, creating: false },
};

export default meta;
type Story = StoryObj<typeof ScheduleForm>;

export const Default: Story = {};

export const Creating: Story = {
  args: { creating: true },
};
