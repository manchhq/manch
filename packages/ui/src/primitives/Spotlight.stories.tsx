import type { Meta, StoryObj } from "@storybook/react";
import { Spotlight } from "./Spotlight";

const meta: Meta<typeof Spotlight> = { title: "primitives/Spotlight", component: Spotlight };
export default meta;
type Story = StoryObj<typeof Spotlight>;

export const Active: Story = { args: { active: true, children: <div className="p-6">in the spotlight</div> } };
export const Dim: Story = { args: { active: false, children: <div className="p-6">offstage</div> } };
