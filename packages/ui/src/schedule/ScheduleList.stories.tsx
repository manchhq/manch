import type { Meta, StoryObj } from "@storybook/react";
import { ScheduleList } from "./ScheduleList";

const meta: Meta<typeof ScheduleList> = {
  title: "Components/Schedule/ScheduleList",
  component: ScheduleList,
};

export default meta;
type Story = StoryObj<typeof ScheduleList>;

export const Default: Story = {
  args: {
    schedules: [
      {
        id: "s1",
        target: "Discovery team",
        cadence: "daily",
        nextRun: "2026-07-01T09:00:00Z",
      },
      {
        id: "s2",
        target: "QA review",
        cadence: "weekly",
        nextRun: "2026-07-07T10:00:00Z",
      },
    ],
  },
};

export const Empty: Story = {
  args: {
    schedules: [],
  },
};
