import { useCallback, useRef } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';
import type {
  ExecutorProfileId,
  WorkspaceRepoInput,
  Workspace,
} from 'shared/types';

type CreateAttemptArgs = {
  profile: ExecutorProfileId;
  repos: WorkspaceRepoInput[];
};

type UseAttemptCreationArgs = {
  taskId: string;
  onSuccess?: (attempt: Workspace) => void;
};

export function useAttemptCreation({
  taskId,
  onSuccess,
}: UseAttemptCreationArgs) {
  const queryClient = useQueryClient();
  const isCreatingRef = useRef(false);

  const mutation = useMutation({
    mutationFn: ({ profile, repos }: CreateAttemptArgs) =>
      attemptsApi.create({
        task_id: taskId,
        executor_profile_id: profile,
        repos,
      }),
    onSuccess: (newAttempt: Workspace) => {
      queryClient.setQueryData(
        ['taskAttempts', taskId],
        (old: Workspace[] = []) => [newAttempt, ...old]
      );
      onSuccess?.(newAttempt);
    },
  });

  const createAttempt = useCallback(
    async (args: CreateAttemptArgs) => {
      if (isCreatingRef.current) return;
      isCreatingRef.current = true;
      try {
        return await mutation.mutateAsync(args);
      } finally {
        isCreatingRef.current = false;
      }
    },
    [mutation]
  );

  return {
    createAttempt,
    isCreating: mutation.isPending,
    error: mutation.error,
  };
}
