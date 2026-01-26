import { useQuery } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type { ProjectWorktreesResponse } from 'shared/types';

export const worktreeKeys = {
  all: ['worktrees'] as const,
  list: (projectId: string) => [...worktreeKeys.all, projectId] as const,
};

type Options = {
  enabled?: boolean;
};

export function useWorktrees(projectId: string | undefined, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!projectId;

  return useQuery<ProjectWorktreesResponse>({
    queryKey: worktreeKeys.list(projectId!),
    queryFn: () => projectsApi.getWorktrees(projectId!),
    enabled,
    staleTime: 30_000, // worktrees change rarely
  });
}
