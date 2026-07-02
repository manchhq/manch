import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import * as api from "../lib/api";
import type { Provider } from "../lib/providers";
import type { CreateWorkspace, CreateTeam, CreateSchedule } from "./bindings";

export function useConfiguredProviders() {
  return useQuery({ queryKey: ["providers"], queryFn: api.listConfiguredProviders });
}
export function useSaveApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ provider, apiKey }: { provider: Provider; apiKey: string }) =>
      api.saveApiKey(provider, apiKey),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

const modelsQueryKey = (provider: Provider) => ["models", provider] as const;

/** Models for a single BYOK provider. `enabled` should gate on: is BYOK + has a saved key. */
export function useModels(provider: Provider, enabled: boolean) {
  return useQuery({
    queryKey: modelsQueryKey(provider),
    queryFn: () => api.listModels(provider),
    enabled,
  });
}

/**
 * Models for however many BYOK providers currently have a saved key.
 * `providerIds` is expected to already be filtered to BYOK providers (see
 * `isByokProvider`/`BYOK_PROVIDERS` in `lib/providers.ts`) — its length can
 * vary across renders, so this uses `useQueries` (rather than mapping
 * `useModels` in a loop) to stay Rules-of-Hooks safe.
 */
export function useModelsForProviders(providerIds: Provider[]) {
  return useQueries({
    queries: providerIds.map((id) => ({ queryKey: modelsQueryKey(id), queryFn: () => api.listModels(id) })),
  });
}

export function useSetModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ provider, model }: { provider: Provider; model: string }) => api.setModel(provider, model),
    onSuccess: (_d, { provider }) => qc.invalidateQueries({ queryKey: modelsQueryKey(provider) }),
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
export function useTeam(id: string | null) {
  return useQuery({ queryKey: ["team", id], queryFn: () => api.getTeam(id!), enabled: !!id });
}
export function useAssignTeamTask() {
  return useMutation({
    mutationFn: ({ teamId, task }: { teamId: string; task: string }) => api.assignTeamTask(teamId, task),
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

/**
 * @param kinds result-kind filter; part of the query key. React Query v5
 * hashes keys structurally, so passing a fresh array literal each render is
 * safe (equal contents → cache hit) — no need to memoize at the call site.
 */
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
