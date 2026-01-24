import React, { createContext, useContext, useMemo } from 'react';
import { useBranchStatus } from '@/hooks';
import { useGitStateSubscription } from '@/hooks/useGitStateSubscription';
import type { RepoBranchStatus, Workspace } from 'shared/types';
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
  workspace: Workspace | undefined;
  children: React.ReactNode;
}> = ({ workspace, children }) => {
  const attemptId = workspace?.id;
  const { data: branchStatus, isLoading, isFetching, refetch } = useBranchStatus(attemptId);

  // Subscribe to git state changes via WebSocket to invalidate branchStatus
  // Only enable when container_ref is set (workspace is ready)
  useGitStateSubscription(attemptId, { enabled: !!workspace?.container_ref });

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
