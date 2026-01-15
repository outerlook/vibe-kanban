import { useQuery } from '@tanstack/react-query';
import { taskGroupsApi } from '@/lib/api';
import type { TaskGroupWithStats } from 'shared/types';

export const taskGroupStatsKeys = {
  all: ['taskGroupStats'] as const,
  byProject: (projectId: string | undefined) =>
    ['taskGroupStats', projectId] as const,
};

type QueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
};

export function useTaskGroupStats(projectId?: string, opts?: QueryOptions) {
  const enabled = (opts?.enabled ?? true) && !!projectId;

  return useQuery<TaskGroupWithStats[]>({
    queryKey: taskGroupStatsKeys.byProject(projectId),
    queryFn: () => taskGroupsApi.getStatsForProject(projectId!),
    enabled,
    staleTime: 30_000,
    refetchInterval: opts?.refetchInterval ?? false,
  });
}
