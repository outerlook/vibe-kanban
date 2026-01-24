import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { attemptsApi, projectsApi, taskGroupsApi, Result } from '@/lib/api';
import type {
  MergeQueueEntry,
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
  groupCount: (groupId: string | undefined) =>
    ['mergeQueue', 'groupCount', groupId] as const,
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
  generateCommitMessage?: boolean;
};

export function useQueueMerge(
  attemptId?: string,
  onSuccess?: (entry: MergeQueueEntry) => void,
  onError?: (err: QueueMergeError | undefined, message?: string) => void
) {
  const queryClient = useQueryClient();

  return useMutation<Result<MergeQueueEntry, QueueMergeError>, unknown, QueueMergeParams>({
    mutationFn: (params: QueueMergeParams) => {
      if (!attemptId) return Promise.resolve({ success: false, error: undefined });
      return attemptsApi.queueMerge(attemptId, {
        repo_id: params.repoId,
        commit_message: params.commitMessage ?? null,
        generate_commit_message: params.generateCommitMessage ?? null,
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

  return useQuery<MergeQueueEntry | null>({
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

export function useGroupQueueCount(groupId?: string, opts?: QueryOptions) {
  const enabled = (opts?.enabled ?? true) && !!groupId;

  return useQuery<MergeQueueCountResponse>({
    queryKey: mergeQueueKeys.groupCount(groupId),
    queryFn: () => taskGroupsApi.getMergeQueueCount(groupId!),
    enabled,
    refetchInterval: opts?.refetchInterval ?? 3000,
    staleTime: opts?.staleTime ?? 2000,
    retry: opts?.retry ?? 2,
  });
}
