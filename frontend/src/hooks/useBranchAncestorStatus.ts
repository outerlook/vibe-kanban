import { useQuery } from '@tanstack/react-query';
import { repoApi } from '@/lib/api';
import type { BranchAncestorStatus } from 'shared/types';

export const branchAncestorKeys = {
  all: ['branchAncestor'] as const,
  byRepoAndBranch: (repoId: string | undefined, branchName: string | undefined) =>
    ['branchAncestor', repoId, branchName] as const,
};

type QueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
};

export function useBranchAncestorStatus(
  repoId?: string,
  branchName?: string,
  opts?: QueryOptions
) {
  const enabled = (opts?.enabled ?? true) && !!repoId && !!branchName;

  return useQuery<BranchAncestorStatus>({
    queryKey: branchAncestorKeys.byRepoAndBranch(repoId, branchName),
    queryFn: () => repoApi.checkBranchAncestor(repoId!, branchName!),
    enabled,
    staleTime: 60_000,
    refetchInterval: opts?.refetchInterval ?? false,
  });
}
