import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { claudeAccountsApi } from '@/lib/api';
import type { SavedAccount } from 'shared/types';

export const claudeAccountsKeys = {
  all: ['claude-accounts'] as const,
  current: ['claude-account-current'] as const,
  currentUuid: ['claude-account-current-uuid'] as const,
};

export function useClaudeAccounts() {
  return useQuery<SavedAccount[]>({
    queryKey: claudeAccountsKeys.all,
    queryFn: claudeAccountsApi.list,
    staleTime: 60 * 1000, // 1 minute
  });
}

export function useCurrentClaudeAccount() {
  return useQuery<string | null>({
    queryKey: claudeAccountsKeys.current,
    queryFn: claudeAccountsApi.getCurrent,
    staleTime: 30 * 1000, // 30 seconds
  });
}

export function useCurrentClaudeAccountUuid() {
  return useQuery<string | null>({
    queryKey: claudeAccountsKeys.currentUuid,
    queryFn: () => claudeAccountsApi.getCurrentUuid(),
    staleTime: 30 * 1000, // 30 seconds
  });
}

export function useSaveClaudeAccount() {
  const queryClient = useQueryClient();

  return useMutation<SavedAccount, unknown, string | undefined>({
    mutationFn: (name) => claudeAccountsApi.saveCurrent(name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: claudeAccountsKeys.all });
      queryClient.invalidateQueries({ queryKey: claudeAccountsKeys.current });
    },
  });
}

export function useSwitchClaudeAccount() {
  const queryClient = useQueryClient();

  return useMutation<void, unknown, string>({
    mutationFn: (hashPrefix) => claudeAccountsApi.switch(hashPrefix),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: claudeAccountsKeys.current });
      queryClient.invalidateQueries({ queryKey: ['account-info'] });
    },
  });
}

export function useUpdateClaudeAccountName() {
  const queryClient = useQueryClient();

  return useMutation<
    SavedAccount,
    unknown,
    { hashPrefix: string; name: string }
  >({
    mutationFn: ({ hashPrefix, name }) =>
      claudeAccountsApi.updateName(hashPrefix, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: claudeAccountsKeys.all });
    },
  });
}

export function useDeleteClaudeAccount() {
  const queryClient = useQueryClient();

  return useMutation<void, unknown, string>({
    mutationFn: (hashPrefix) => claudeAccountsApi.delete(hashPrefix),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: claudeAccountsKeys.all });
      queryClient.invalidateQueries({ queryKey: claudeAccountsKeys.current });
    },
  });
}
