import { useState, useEffect, useCallback } from 'react';

const STORAGE_KEY_PREFIX = 'vk-pr-base-branch-';

function getStorageKey(projectId: string): string {
  return `${STORAGE_KEY_PREFIX}${projectId}`;
}

export function useBaseBranchPreference(
  projectId: string
): [string | null, (branch: string | null) => void] {
  const [baseBranch, setBaseBranchState] = useState<string | null>(() => {
    try {
      return localStorage.getItem(getStorageKey(projectId));
    } catch {
      return null;
    }
  });

  // Re-read from localStorage when projectId changes
  useEffect(() => {
    try {
      setBaseBranchState(localStorage.getItem(getStorageKey(projectId)));
    } catch {
      setBaseBranchState(null);
    }
  }, [projectId]);

  const setBaseBranch = useCallback(
    (branch: string | null) => {
      setBaseBranchState(branch);
      try {
        const key = getStorageKey(projectId);
        if (branch === null) {
          localStorage.removeItem(key);
        } else {
          localStorage.setItem(key, branch);
        }
      } catch {
        // Ignore storage errors
      }
    },
    [projectId]
  );

  return [baseBranch, setBaseBranch];
}
