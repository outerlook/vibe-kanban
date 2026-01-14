import { useQuery } from '@tanstack/react-query';
import { accountInfoApi } from '@/lib/api';
import type { AccountInfo } from 'shared/types';

export function useAccountInfo() {
  return useQuery<AccountInfo>({
    queryKey: ['account-info'],
    queryFn: accountInfoApi.get,
    staleTime: 5 * 60 * 1000,
    retry: false,
  });
}
