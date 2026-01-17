import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import type { TaskStatus } from 'shared/types';
import { TASK_STATUSES } from '@/constants/taskStatuses';

const PARAM_KEYS = {
  search: 'q',
  group: 'group',
  status: 'status',
  blocked: 'blocked',
} as const;

export interface TaskFilters {
  search: string;
  groupId: string | null;
  statuses: TaskStatus[];
  hideBlocked: boolean;
}

export interface TaskFiltersHook {
  filters: TaskFilters;
  setSearch: (query: string) => void;
  setGroupId: (groupId: string | null) => void;
  setStatuses: (statuses: TaskStatus[]) => void;
  setHideBlocked: (value: boolean) => void;
  clearFilters: () => void;
  hasActiveFilters: boolean;
}

function parseStatuses(value: string | null): TaskStatus[] {
  if (!value) return [];
  return value
    .split(',')
    .map((s) => s.trim().toLowerCase())
    .filter((s): s is TaskStatus =>
      TASK_STATUSES.includes(s as TaskStatus)
    );
}

function serializeStatuses(statuses: TaskStatus[]): string {
  return statuses.join(',');
}

export function useTaskFilters(): TaskFiltersHook {
  const [searchParams, setSearchParams] = useSearchParams();

  const filters = useMemo<TaskFilters>(() => {
    const search = searchParams.get(PARAM_KEYS.search) ?? '';
    const groupId = searchParams.get(PARAM_KEYS.group);
    const statuses = parseStatuses(searchParams.get(PARAM_KEYS.status));
    const hideBlocked = searchParams.get(PARAM_KEYS.blocked) === 'hide';

    return { search, groupId, statuses, hideBlocked };
  }, [searchParams]);

  const setSearch = useCallback(
    (query: string) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (query.trim()) {
            next.set(PARAM_KEYS.search, query);
          } else {
            next.delete(PARAM_KEYS.search);
          }
          return next;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  const setGroupId = useCallback(
    (groupId: string | null) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (groupId) {
            next.set(PARAM_KEYS.group, groupId);
          } else {
            next.delete(PARAM_KEYS.group);
          }
          return next;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  const setStatuses = useCallback(
    (statuses: TaskStatus[]) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          const validStatuses = statuses.filter((s) =>
            TASK_STATUSES.includes(s)
          );
          if (validStatuses.length > 0) {
            next.set(PARAM_KEYS.status, serializeStatuses(validStatuses));
          } else {
            next.delete(PARAM_KEYS.status);
          }
          return next;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  const setHideBlocked = useCallback(
    (value: boolean) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (value) {
            next.set(PARAM_KEYS.blocked, 'hide');
          } else {
            next.delete(PARAM_KEYS.blocked);
          }
          return next;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  const clearFilters = useCallback(() => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete(PARAM_KEYS.search);
        next.delete(PARAM_KEYS.group);
        next.delete(PARAM_KEYS.status);
        next.delete(PARAM_KEYS.blocked);
        return next;
      },
      { replace: true }
    );
  }, [setSearchParams]);

  const hasActiveFilters = useMemo(() => {
    return (
      filters.search.trim().length > 0 ||
      filters.groupId !== null ||
      filters.hideBlocked
    );
  }, [filters.search, filters.groupId, filters.hideBlocked]);

  return {
    filters,
    setSearch,
    setGroupId,
    setStatuses,
    setHideBlocked,
    clearFilters,
    hasActiveFilters,
  };
}
