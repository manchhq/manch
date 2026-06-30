import { invoke } from "@tauri-apps/api/core";
import type { Provider } from "./providers";
import type { Workspace, CreateWorkspace, Team, CreateTeam, TeamRun, Schedule, CreateSchedule, SearchHit, CrossVerify } from "../data/bindings";

export const saveApiKey = (provider: Provider, apiKey: string): Promise<void> =>
  invoke("save_api_key", { provider, apiKey });
export const listConfiguredProviders = (): Promise<Provider[]> => invoke("list_configured_providers");
export const sendPrompt = (provider: Provider, text: string): Promise<string> => invoke("send_prompt", { provider, text });

export const listWorkspaces = (): Promise<Workspace[]> => invoke("list_workspaces");
export const createWorkspace = (input: CreateWorkspace): Promise<Workspace> => invoke("create_workspace", { input });
export const renameWorkspace = (id: string, name: string): Promise<Workspace> => invoke("rename_workspace", { id, name });
export const deleteWorkspace = (id: string): Promise<void> => invoke("delete_workspace", { id });

export const listTeams = (workspaceId: string): Promise<Team[]> => invoke("list_teams", { workspaceId });
export const createTeam = (input: CreateTeam): Promise<Team> => invoke("create_team", { input });
export const getTeam = (id: string): Promise<Team> => invoke("get_team", { id });
export const assignTeamTask = (teamId: string, task: string): Promise<TeamRun> => invoke("assign_team_task", { teamId, task });

export const listSchedules = (workspaceId: string): Promise<Schedule[]> => invoke("list_schedules", { workspaceId });
export const createSchedule = (input: CreateSchedule): Promise<Schedule> => invoke("create_schedule", { input });

export const search = (workspaceId: string, query: string, kinds: string[]): Promise<SearchHit[]> =>
  invoke("search", { workspaceId, query, kinds });
export const crossVerify = (providers: string[], text: string): Promise<CrossVerify> =>
  invoke("cross_verify", { providers, text });
