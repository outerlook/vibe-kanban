import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { githubSettingsApi } from '@/lib/api';
import type { GitHubSettingsStatus, GitHubImportResponse } from 'shared/types';

const githubSettingsKey = ['github', 'settings'] as const;

export function useGitHubSettings() {
  return useQuery<GitHubSettingsStatus>({
    queryKey: githubSettingsKey,
    queryFn: () => githubSettingsApi.getStatus(),
    staleTime: 30_000,
  });
}

export function useSetGitHubToken() {
  const queryClient = useQueryClient();

  return useMutation<GitHubSettingsStatus, Error, string>({
    mutationFn: (token: string) => githubSettingsApi.setToken(token),
    onSuccess: (data) => {
      queryClient.setQueryData(githubSettingsKey, data);
    },
  });
}

export function useDeleteGitHubToken() {
  const queryClient = useQueryClient();

  return useMutation<GitHubSettingsStatus, Error, void>({
    mutationFn: () => githubSettingsApi.deleteToken(),
    onSuccess: (data) => {
      queryClient.setQueryData(githubSettingsKey, data);
    },
  });
}

export function useImportGitHubToken() {
  const queryClient = useQueryClient();

  return useMutation<GitHubImportResponse, Error, void>({
    mutationFn: () => githubSettingsApi.importFromCli(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: githubSettingsKey });
    },
  });
}
