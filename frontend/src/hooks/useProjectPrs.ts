import { useQuery } from '@tanstack/react-query';
import { projectsApi, ProjectPrsResponse } from '@/lib/api';

export const prKeys = {
  all: ['pullRequests'] as const,
  byProject: (projectId: string | undefined) =>
    ['pullRequests', 'project', projectId] as const,
};

type Options = {
  enabled?: boolean;
  refetchInterval?: number | false;
  staleTime?: number;
};

export function useProjectPrs(projectId?: string, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!projectId;

  return useQuery<ProjectPrsResponse>({
    queryKey: prKeys.byProject(projectId),
    queryFn: () => projectsApi.getPullRequests(projectId!),
    enabled,
    staleTime: opts?.staleTime ?? 30_000, // 30s - PRs don't change that fast
    refetchInterval: opts?.refetchInterval ?? 60_000, // 60s auto-refresh
    retry: 2,
  });
}
