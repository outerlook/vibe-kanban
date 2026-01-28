import { useQuery } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';
import type { Workspace } from 'shared/types';

export const taskAttemptKeys = {
  all: ['taskAttempts'] as const,
  byTask: (taskId: string | undefined) => ['taskAttempts', taskId] as const,
};

type Options = {
  enabled?: boolean;
  refetchInterval?: number | false;
};

export function useTaskAttempts(taskId?: string, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!taskId;

  return useQuery<Workspace[]>({
    queryKey: taskAttemptKeys.byTask(taskId),
    queryFn: () => attemptsApi.getAll(taskId!),
    enabled,
    refetchInterval: (query) => {
      if (opts?.refetchInterval !== undefined) return opts.refetchInterval;
      const data = query.state.data;
      // Poll while any workspace is still setting up (setup_completed_at is null)
      if (data?.some((workspace) => workspace.setup_completed_at === null)) {
        return 5000;
      }
      return false;
    },
  });
}
