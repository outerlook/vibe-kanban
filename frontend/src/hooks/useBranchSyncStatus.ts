import { useQuery } from '@tanstack/react-query';
import { repoApi } from '@/lib/api';
import type { BatchBranchSyncStatus, BranchSyncStatus } from 'shared/types';

export const branchSyncStatusKeys = {
  all: ['branchSyncStatus'] as const,
  byRepoAndBranch: (repoId: string | undefined, branchName: string | undefined, projectId: string | undefined) =>
    ['branchSyncStatus', repoId, branchName, projectId] as const,
  batch: (repoId: string | undefined, projectId: string | undefined, branches: string[]) =>
    ['branchSyncStatus', 'batch', repoId, projectId, branches.slice().sort().join(',')] as const,
};

type QueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
};

export function useBranchSyncStatus(
  repoId?: string,
  branchName?: string,
  projectId?: string,
  opts?: QueryOptions
) {
  const enabled = (opts?.enabled ?? true) && !!repoId && !!branchName && !!projectId;

  return useQuery<BranchSyncStatus>({
    queryKey: branchSyncStatusKeys.byRepoAndBranch(repoId, branchName, projectId),
    queryFn: () => repoApi.checkBranchSyncStatus(repoId!, {
      branch_name: branchName!,
      project_id: projectId!,
    }),
    enabled,
    staleTime: 30_000,
    refetchInterval: opts?.refetchInterval ?? false,
  });
}

export function useBatchBranchSyncStatus(
  repoId?: string,
  projectId?: string,
  branches?: string[],
  opts?: QueryOptions
) {
  const enabled = (opts?.enabled ?? true) && !!repoId && !!projectId && !!branches && branches.length > 0;

  return useQuery<BatchBranchSyncStatus>({
    queryKey: branchSyncStatusKeys.batch(repoId, projectId, branches ?? []),
    queryFn: () => repoApi.batchCheckBranchSyncStatus(repoId!, {
      branches: branches!,
      project_id: projectId!,
    }),
    enabled,
    staleTime: 30_000,
    refetchInterval: opts?.refetchInterval ?? false,
  });
}
