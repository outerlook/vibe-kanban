import { useQuery } from '@tanstack/react-query';
import { repoApi } from '@/lib/api';
import type { BatchBranchMergeStatus, BranchMergeStatus } from 'shared/types';

export const branchMergeStatusKeys = {
  all: ['branchMergeStatus'] as const,
  byRepoAndBranch: (repoId: string | undefined, branchName: string | undefined, projectId: string | undefined) =>
    ['branchMergeStatus', repoId, branchName, projectId] as const,
  batch: (repoId: string | undefined, projectId: string | undefined, branches: string[]) =>
    ['branchMergeStatus', 'batch', repoId, projectId, branches] as const,
};

type QueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
};

export function useBranchMergeStatus(
  repoId?: string,
  branchName?: string,
  projectId?: string,
  opts?: QueryOptions
) {
  const enabled = (opts?.enabled ?? true) && !!repoId && !!branchName && !!projectId;

  return useQuery<BranchMergeStatus>({
    queryKey: branchMergeStatusKeys.byRepoAndBranch(repoId, branchName, projectId),
    queryFn: () => repoApi.checkBranchMergeStatus(repoId!, branchName!, projectId!),
    enabled,
    staleTime: 60_000,
    refetchInterval: opts?.refetchInterval ?? false,
  });
}

export function useBatchBranchMergeStatus(
  repoId?: string,
  projectId?: string,
  branches?: string[],
  opts?: QueryOptions
) {
  const enabled = (opts?.enabled ?? true) && !!repoId && !!projectId && !!branches && branches.length > 0;

  return useQuery<BatchBranchMergeStatus>({
    queryKey: branchMergeStatusKeys.batch(repoId, projectId, branches ?? []),
    queryFn: () => repoApi.batchCheckBranchMergeStatus(repoId!, branches!, projectId!),
    enabled,
    staleTime: 60_000,
    refetchInterval: opts?.refetchInterval ?? false,
  });
}
