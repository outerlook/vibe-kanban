import { useMemo } from 'react';
import { useQueries } from '@tanstack/react-query';
import { tasksApi } from '@/lib/api';
import type { Task } from 'shared/types';
import { taskKeys } from './useTask';

type UseCanBulkCreateAttemptsResult = {
  canCreate: boolean;
  hasMixedGroups: boolean;
  isLoading: boolean;
};

export function useCanBulkCreateAttempts(
  taskIds: string[]
): UseCanBulkCreateAttemptsResult {
  const taskQueries = useQueries({
    queries: taskIds.map((taskId) => ({
      queryKey: taskKeys.byId(taskId),
      queryFn: () => tasksApi.getById(taskId),
      enabled: !!taskId,
    })),
  });

  const isLoading = taskQueries.some((q) => q.isLoading);

  const hasMixedGroups = useMemo(() => {
    if (isLoading) return false;

    const loadedTasks = taskQueries
      .map((q) => q.data)
      .filter((t): t is Task => t !== undefined);

    if (loadedTasks.length < 2) return false;

    const groupIds = loadedTasks.map((t) => t.task_group_id);
    const uniqueGroupIds = new Set(groupIds);

    return uniqueGroupIds.size > 1;
  }, [taskQueries, isLoading]);

  return {
    canCreate: !isLoading && !hasMixedGroups,
    hasMixedGroups,
    isLoading,
  };
}
