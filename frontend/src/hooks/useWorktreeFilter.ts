import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

const PARAM_KEY = 'worktree';

export const WORKTREE_FILTER_VALUES = {
  ALL: '__all__',
  MAIN: '__main__',
} as const;

export interface WorktreeFilterHook {
  selectedWorktree: string;
  setSelectedWorktree: (value: string) => void;
  getWorktreePathForApi: () => string | undefined;
}

export function useWorktreeFilter(): WorktreeFilterHook {
  const [searchParams, setSearchParams] = useSearchParams();

  const selectedWorktree = useMemo(() => {
    return searchParams.get(PARAM_KEY) ?? WORKTREE_FILTER_VALUES.ALL;
  }, [searchParams]);

  const setSelectedWorktree = useCallback(
    (value: string) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (value === WORKTREE_FILTER_VALUES.ALL) {
            next.delete(PARAM_KEY);
          } else {
            next.set(PARAM_KEY, value);
          }
          return next;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  const getWorktreePathForApi = useCallback(() => {
    if (selectedWorktree === WORKTREE_FILTER_VALUES.ALL) {
      return undefined;
    }
    if (selectedWorktree === WORKTREE_FILTER_VALUES.MAIN) {
      return WORKTREE_FILTER_VALUES.MAIN;
    }
    return selectedWorktree;
  }, [selectedWorktree]);

  return {
    selectedWorktree,
    setSelectedWorktree,
    getWorktreePathForApi,
  };
}
