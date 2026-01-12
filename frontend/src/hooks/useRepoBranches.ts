import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { repoApi } from '@/lib/api';
import type { GitBranch } from 'shared/types';

export const repoBranchKeys = {
  all: ['repoBranches'] as const,
  byRepo: (repoId: string | undefined) => ['repoBranches', repoId] as const,
};

type Options = {
  enabled?: boolean;
};

export function useRepoBranches(repoId?: string | null, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!repoId;

  return useQuery<GitBranch[]>({
    queryKey: repoBranchKeys.byRepo(repoId ?? undefined),
    queryFn: () => repoApi.getBranches(repoId!),
    enabled,
    staleTime: 60_000,
    refetchOnWindowFocus: true,
  });
}

export function useCreateBranch(repoId: string) {
  const queryClient = useQueryClient();

  return useMutation<GitBranch, unknown, string>({
    mutationFn: (name: string) => repoApi.createBranch(repoId, name),
    onSuccess: () => {
      // Invalidating the parent key covers all child keys including byRepo(repoId)
      queryClient.invalidateQueries({
        queryKey: repoBranchKeys.all,
      });
    },
  });
}
