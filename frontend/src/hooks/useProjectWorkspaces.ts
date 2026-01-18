import { useQuery } from '@tanstack/react-query';
import { projectsApi } from '@/lib/api';
import type { Workspace } from 'shared/types';

export const workspaceKeys = {
  all: ['workspaces'] as const,
  byProject: (projectId: string | undefined) =>
    ['workspaces', 'project', projectId] as const,
};

type Options = {
  enabled?: boolean;
};

export function useProjectWorkspaces(projectId?: string, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!projectId;

  return useQuery<Workspace[]>({
    queryKey: workspaceKeys.byProject(projectId),
    queryFn: () => projectsApi.getWorkspaces(projectId!),
    enabled,
    staleTime: 60_000, // 60s - workspaces don't change often during PR review
    retry: 2,
  });
}
