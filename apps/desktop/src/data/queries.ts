import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import * as api from "../lib/api";
import type { CreateWorkspace, CreateTeam, CreateSchedule } from "./bindings";

export function useConfiguredProviders() {
  return useQuery({ queryKey: ["providers"], queryFn: api.listConfiguredProviders });
}
export function useSaveApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ provider, apiKey }: { provider: import("../lib/providers").Provider; apiKey: string }) =>
      api.saveApiKey(provider, apiKey),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

export function useWorkspaces() {
  return useQuery({ queryKey: ["workspaces"], queryFn: api.listWorkspaces });
}
export function useCreateWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateWorkspace) => api.createWorkspace(input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });
}
export function useRenameWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => api.renameWorkspace(id, name),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });
}
export function useDeleteWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });
}

export function useTeams(workspaceId: string | null) {
  return useQuery({ queryKey: ["teams", workspaceId], queryFn: () => api.listTeams(workspaceId!), enabled: !!workspaceId });
}
export function useCreateTeam() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateTeam) => api.createTeam(input),
    onSuccess: (_d, input) => qc.invalidateQueries({ queryKey: ["teams", input.workspace_id] }),
  });
}

export function useSchedules(workspaceId: string | null) {
  return useQuery({ queryKey: ["schedules", workspaceId], queryFn: () => api.listSchedules(workspaceId!), enabled: !!workspaceId });
}
export function useCreateSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateSchedule) => api.createSchedule(input),
    onSuccess: (_d, input) => qc.invalidateQueries({ queryKey: ["schedules", input.workspace_id] }),
  });
}

export function useSearch(workspaceId: string | null, query: string, kinds: string[]) {
  return useQuery({
    queryKey: ["search", workspaceId, query, kinds],
    queryFn: () => api.search(workspaceId!, query, kinds),
    enabled: !!workspaceId && query.length > 0,
  });
}
export function useCrossVerify() {
  return useMutation({
    mutationFn: ({ providers, text }: { providers: string[]; text: string }) =>
      api.crossVerify(providers, text),
  });
}
