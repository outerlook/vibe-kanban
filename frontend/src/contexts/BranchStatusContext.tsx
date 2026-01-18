import React, { createContext, useContext, useMemo } from 'react';
import { useBranchStatus } from '@/hooks';
import type { RepoBranchStatus } from 'shared/types';
import type {
  QueryObserverResult,
  RefetchOptions,
} from '@tanstack/react-query';

type BranchStatusContextType = {
  branchStatus: RepoBranchStatus[] | undefined;
  isLoading: boolean;
  isFetching: boolean;
  refetch: (
    options?: RefetchOptions
  ) => Promise<QueryObserverResult<RepoBranchStatus[], Error>>;
};

const BranchStatusContext = createContext<BranchStatusContextType | null>(null);

export const BranchStatusProvider: React.FC<{
  attemptId: string | undefined;
  children: React.ReactNode;
}> = ({ attemptId, children }) => {
  const { data: branchStatus, isLoading, isFetching, refetch } = useBranchStatus(attemptId);

  const value = useMemo<BranchStatusContextType>(
    () => ({
      branchStatus,
      isLoading,
      isFetching,
      refetch,
    }),
    [branchStatus, isLoading, isFetching, refetch]
  );

  return (
    <BranchStatusContext.Provider value={value}>
      {children}
    </BranchStatusContext.Provider>
  );
};

export const useBranchStatusContext = () => {
  const ctx = useContext(BranchStatusContext);
  if (!ctx) {
    throw new Error(
      'useBranchStatusContext must be used within BranchStatusProvider'
    );
  }
  return ctx;
};
