import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { attemptsApi, projectsApi, Result } from '@/lib/api';
import type {
  MergeQueue,
  MergeQueueCountResponse,
  QueueMergeError,
} from 'shared/types';
import { repoBranchKeys } from './useRepoBranches';

export const mergeQueueKeys = {
  all: ['mergeQueue'] as const,
  status: (workspaceId: string | undefined) =>
    ['mergeQueue', 'status', workspaceId] as const,
  projectCount: (projectId: string | undefined) =>
    ['mergeQueue', 'projectCount', projectId] as const,
};

type QueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
  staleTime?: number;
  retry?: number | false;
};

type QueueMergeParams = {
  repoId: string;
  commitMessage?: string;
};

export function useQueueMerge(
  attemptId?: string,
  onSuccess?: (entry: MergeQueue) => void,
  onError?: (err: QueueMergeError | undefined, message?: string) => void
) {
  const queryClient = useQueryClient();

  return useMutation<Result<MergeQueue, QueueMergeError>, unknown, QueueMergeParams>({
    mutationFn: (params: QueueMergeParams) => {
      if (!attemptId) return Promise.resolve({ success: false, error: undefined });
      return attemptsApi.queueMerge(attemptId, {
        repo_id: params.repoId,
        commit_message: params.commitMessage ?? null,
      });
    },
    onSuccess: (result) => {
      if (result.success) {
        queryClient.invalidateQueries({
          queryKey: mergeQueueKeys.status(attemptId),
        });
        queryClient.invalidateQueries({ queryKey: mergeQueueKeys.all });
        queryClient.invalidateQueries({ queryKey: repoBranchKeys.all });
        onSuccess?.(result.data);
      } else {
        onError?.(result.error, result.message);
      }
    },
    onError: (err) => {
      console.error('Failed to queue merge:', err);
      onError?.(undefined, String(err));
    },
  });
}

export function useCancelQueuedMerge(
  attemptId?: string,
  onSuccess?: () => void,
  onError?: (err: unknown) => void
) {
  const queryClient = useQueryClient();

  return useMutation<void, unknown, void>({
    mutationFn: () => {
      if (!attemptId) return Promise.resolve();
      return attemptsApi.cancelQueuedMerge(attemptId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: mergeQueueKeys.status(attemptId),
      });
      queryClient.invalidateQueries({ queryKey: mergeQueueKeys.all });
      onSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to cancel queued merge:', err);
      onError?.(err);
    },
  });
}

export function useQueueStatus(workspaceId?: string, opts?: QueryOptions) {
  const enabled = (opts?.enabled ?? true) && !!workspaceId;

  return useQuery<MergeQueue | null>({
    queryKey: mergeQueueKeys.status(workspaceId),
    queryFn: () => attemptsApi.getQueueStatus(workspaceId!),
    enabled,
    refetchInterval: (query) => {
      if (opts?.refetchInterval !== undefined) return opts.refetchInterval;
      const data = query.state.data;
      if (data && (data.status === 'queued' || data.status === 'merging')) {
        return 2000;
      }
      return false;
    },
    staleTime: opts?.staleTime ?? 5000,
    retry: opts?.retry ?? 2,
  });
}

export function useProjectQueueCount(projectId?: string, opts?: QueryOptions) {
  const enabled = (opts?.enabled ?? true) && !!projectId;

  return useQuery<MergeQueueCountResponse>({
    queryKey: mergeQueueKeys.projectCount(projectId),
    queryFn: () => projectsApi.getMergeQueueCount(projectId!),
    enabled,
    refetchInterval: opts?.refetchInterval ?? 3000,
    staleTime: opts?.staleTime ?? 2000,
    retry: opts?.retry ?? 2,
  });
}
