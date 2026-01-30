import { useQuery } from '@tanstack/react-query';
import { tasksApi } from '@/lib/api';
import { taskKeys } from '@/lib/taskCacheHelpers';
import type { Task } from 'shared/types';

type Options = {
  enabled?: boolean;
};

export function useTask(taskId?: string, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!taskId;

  return useQuery<Task>({
    queryKey: taskKeys.byId(taskId),
    queryFn: () => tasksApi.getById(taskId!),
    enabled,
  });
}
