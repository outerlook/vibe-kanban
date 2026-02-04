import { useMutation, useQueryClient } from '@tanstack/react-query';
import { repoApi } from '@/lib/api';
import { branchSyncStatusKeys } from './useBranchSyncStatus';
import type { PushBranchError } from 'shared/types';

export interface PushBranchParams {
  repoId: string;
  branchName: string;
  force?: boolean;
}

class PushBranchErrorWithData extends Error {
  constructor(
    message: string,
    public errorData?: PushBranchError
  ) {
    super(message);
    this.name = 'PushBranchErrorWithData';
  }
}

export function usePushBranch(
  onSuccess?: () => void,
  onError?: (err: unknown, errorData?: PushBranchError) => void
) {
  const queryClient = useQueryClient();

  return useMutation<void, unknown, PushBranchParams>({
    mutationFn: async ({ repoId, branchName, force = false }: PushBranchParams) => {
      const result = await repoApi.pushBranch(repoId, {
        branch_name: branchName,
        force,
      });
      if (!result.success) {
        throw new PushBranchErrorWithData(
          result.message || 'Push failed',
          result.error
        );
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: branchSyncStatusKeys.all });
      onSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to push branch:', err);
      const errorData =
        err instanceof PushBranchErrorWithData ? err.errorData : undefined;
      onError?.(err, errorData);
    },
  });
}
