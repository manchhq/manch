import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listConfiguredProviders, saveApiKey } from "../lib/api";

export function useConfiguredProviders() {
  return useQuery({ queryKey: ["providers"], queryFn: () => listConfiguredProviders() });
}

export function useSaveApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ provider, apiKey }: { provider: string; apiKey: string }) =>
      saveApiKey(provider as never, apiKey),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}
