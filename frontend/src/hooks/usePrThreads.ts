import { useQuery } from '@tanstack/react-query';
import { projectsApi, PrThreadsResponse } from '@/lib/api';

export const prThreadsKeys = {
  all: ['prThreads'] as const,
  byPr: (projectId: string | undefined, repoId: string | undefined, prNumber: number | undefined) =>
    ['prThreads', 'pr', projectId, repoId, prNumber] as const,
};

type Options = {
  enabled?: boolean;
  staleTime?: number;
};

export function usePrThreads(
  projectId?: string,
  repoId?: string,
  prNumber?: number,
  opts?: Options
) {
  const hasRequiredParams = !!projectId && !!repoId && prNumber !== undefined;
  const enabled = (opts?.enabled ?? true) && hasRequiredParams;

  return useQuery<PrThreadsResponse>({
    queryKey: prThreadsKeys.byPr(projectId, repoId, prNumber),
    queryFn: () => projectsApi.getPrThreads(projectId!, repoId!, prNumber!),
    enabled,
    staleTime: opts?.staleTime ?? 30_000,
    retry: 2,
  });
}
