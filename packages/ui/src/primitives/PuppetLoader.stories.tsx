import type { Meta, StoryObj } from "@storybook/react";
import { PuppetLoader } from "./PuppetLoader";

const meta: Meta<typeof PuppetLoader> = { title: "primitives/PuppetLoader", component: PuppetLoader };
export default meta;
type Story = StoryObj<typeof PuppetLoader>;

export const Male: Story = { args: { variant: "male", label: "Consulting the AIs…" } };
export const Female: Story = { args: { variant: "female", label: "Consulting the AIs…" } };
export const Large: Story = { args: { size: 96, variant: "male", label: "Streaming" } };
export const WithLabel: Story = { args: { label: "Thinking…" } };
