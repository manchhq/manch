export type Role = "user" | "agent";
export type ToolStatus = "running" | "done" | "error";
export type AgentStatus = "idle" | "busy" | "done" | "error";

export interface MessageData {
  id: string;
  role: Role;
  text: string; // markdown
}

export interface ToolCallData {
  id: string;
  name: string;
  status: ToolStatus;
  detail?: string;
}

export interface ProviderOption {
  id: string;
  label: string;
}

export interface ConversationSummary {
  id: string;
  title: string;
}
