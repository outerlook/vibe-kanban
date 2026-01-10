import { useMutation } from '@tanstack/react-query';
import { attemptsApi } from '@/lib/api';
import type { GenerateCommitMessageResponse } from 'shared/types';

type GenerateCommitMessageParams = {
  repoId: string;
};

export function useGenerateCommitMessage(attemptId?: string) {
  return useMutation<GenerateCommitMessageResponse, unknown, GenerateCommitMessageParams>({
    mutationFn: (params: GenerateCommitMessageParams) => {
      if (!attemptId) {
        return Promise.reject(new Error('No attempt ID provided'));
      }
      return attemptsApi.generateCommitMessage(attemptId, {
        repo_id: params.repoId,
      });
    },
  });
}
