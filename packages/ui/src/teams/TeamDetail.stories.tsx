import type { StoryObj } from "@storybook/react";
import { TeamDetail } from "./TeamDetail";

export default {
  title: "teams/TeamDetail",
  component: TeamDetail,
};

type Story = StoryObj<typeof TeamDetail>;

export const NoRun: Story = {
  args: {
    name: "Discovery",
    problem: "find precedent cases",
    members: [
      { role: "Researcher", provider: "anthropic" },
      { role: "Analyzer", provider: "anthropic" },
    ],
    capabilities: ["read_file", "search", "browse_web", "analyze_code"],
    onAssign: (task) => console.log("Assigning task:", task),
  },
};

export const WithRun: Story = {
  args: {
    name: "Discovery",
    problem: "find precedent cases",
    members: [
      { role: "Researcher", provider: "anthropic" },
      { role: "Analyzer", provider: "anthropic" },
    ],
    capabilities: ["read_file", "search", "browse_web", "analyze_code"],
    run: {
      task: "Find recent precedent for contract interpretation",
      steps: [
        { memberRole: "Researcher", detail: "searching legal databases", status: "done" },
        { memberRole: "Analyzer", detail: "reviewing case relevance", status: "done" },
        { memberRole: "Researcher", detail: "compiling brief summary", status: "done" },
      ],
      result: "Found 3 precedent cases from 2023-2024 supporting interpretive doctrine.",
    },
    onAssign: (task) => console.log("Assigning task:", task),
  },
};
