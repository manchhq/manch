import type { Meta, StoryObj } from "@storybook/react";
import { Composer } from "./Composer";

const meta: Meta<typeof Composer> = { title: "stage/Composer", component: Composer };
export default meta;
type Story = StoryObj<typeof Composer>;

export const Empty: Story = { args: { value: "", onChange: () => {}, onSend: () => {} } };
export const Typed: Story = { args: { value: "refactor the parser", onChange: () => {}, onSend: () => {} } };
export const Busy: Story = { args: { value: "refactor the parser", busy: true, onChange: () => {}, onSend: () => {} } };
