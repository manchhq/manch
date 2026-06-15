import type { Meta, StoryObj } from "@storybook/react";
import { VersionBadge } from "./VersionBadge";

const meta: Meta<typeof VersionBadge> = {
  title: "Components/VersionBadge",
  component: VersionBadge,
};

export default meta;
type Story = StoryObj<typeof VersionBadge>;

export const Loaded: Story = { args: { version: "0.0.0" } };
export const Loading: Story = { args: { version: undefined, loading: true } };
export const Errored: Story = { args: { version: undefined, error: "unreachable" } };
