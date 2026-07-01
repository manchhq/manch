import type { Meta, StoryObj } from "@storybook/react";
import { PuppetLoader } from "./PuppetLoader";

const meta: Meta<typeof PuppetLoader> = { title: "primitives/PuppetLoader", component: PuppetLoader };
export default meta;
type Story = StoryObj<typeof PuppetLoader>;

export const Default: Story = {};
export const WithLabel: Story = { args: { label: "Consulting the AIs…" } };
export const Large: Story = { args: { size: 96, label: "Streaming" } };
