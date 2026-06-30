import type { Meta, StoryObj } from "@storybook/react";
import { ToolCallCard } from "./ToolCallCard";

const meta: Meta<typeof ToolCallCard> = { title: "stage/ToolCallCard", component: ToolCallCard };
export default meta;
type Story = StoryObj<typeof ToolCallCard>;

export const Running: Story = { args: { call: { id: "1", name: "read_file", status: "running", detail: "parser.rs" } } };
export const Done: Story = { args: { call: { id: "2", name: "read_file", status: "done", detail: "parser.rs" } } };
export const Errored: Story = { args: { call: { id: "3", name: "edit_file", status: "error", detail: "permission denied" } } };
