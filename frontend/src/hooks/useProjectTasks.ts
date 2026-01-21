import { useCallback, useEffect, useMemo, useState } from 'react';
import { useQueries, useQueryClient } from '@tanstack/react-query';
import { useAuth } from '@/hooks';
import { useProject } from '@/contexts/ProjectContext';
import { useLiveQuery, eq, isNull } from '@tanstack/react-db';
import { sharedTasksCollection } from '@/lib/electric/sharedTasksCollection';
import { useAssigneeUserNames } from './useAssigneeUserName';
import { useAutoLinkSharedTasks } from './useAutoLinkSharedTasks';
import { tasksApi, getApiBaseUrlSync } from '@/lib/api';
import type { Operation } from 'rfc6902';
import type {
  SharedTask,
  TaskStatus,
  TaskWithAttemptStatus,
} from 'shared/types';

export const projectTasksKeys = {
  all: ['projectTasks'] as const,
  byProject: (projectId: string | undefined) =>
    ['projectTasks', projectId] as const,
  byProjectInfinite: (projectId: string | undefined) =>
    ['projectTasks', 'infinite', projectId] as const,
  byProjectAndStatus: (projectId: string | undefined, status: TaskStatus) =>
    ['projectTasks', projectId, 'status', status] as const,
};

export type SharedTaskRecord = SharedTask & {
  remote_project_id: string;
  assignee_first_name?: string | null;
  assignee_last_name?: string | null;
  assignee_username?: string | null;
};

type TasksState = {
  tasks: Record<string, TaskWithAttemptStatus>;
};

const PAGE_SIZE = 25;

const ALL_STATUSES: TaskStatus[] = ['todo', 'inprogress', 'inreview', 'done', 'cancelled'];

type StatusPaginationState = {
  offset: number;
  total: number;
  hasMore: boolean;
  isLoading: boolean;
};

type PerStatusPagination = Record<TaskStatus, StatusPaginationState>;

const getOrderByForStatus = (status: TaskStatus): 'created_at_asc' | 'updated_at_desc' => {
  // Done/cancelled: newest completed first (updated_at desc)
  // Others: oldest pending first (created_at asc)
  return status === 'done' || status === 'cancelled' ? 'updated_at_desc' : 'created_at_asc';
};

const sortTasksForStatus = <T extends { created_at: string | Date; updated_at: string | Date }>(
  tasks: T[],
  status: TaskStatus
): void => {
  const orderBy = getOrderByForStatus(status);
  if (orderBy === 'updated_at_desc') {
    tasks.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
  } else {
    tasks.sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime());
  }
};

const createInitialPaginationState = (): PerStatusPagination => ({
  todo: { offset: 0, total: 0, hasMore: false, isLoading: false },
  inprogress: { offset: 0, total: 0, hasMore: false, isLoading: false },
  inreview: { offset: 0, total: 0, hasMore: false, isLoading: false },
  done: { offset: 0, total: 0, hasMore: false, isLoading: false },
  cancelled: { offset: 0, total: 0, hasMore: false, isLoading: false },
});

const TASK_PATH_PREFIX = '/tasks/';

type WsJsonPatchMsg = { JsonPatch: Operation[] };
type WsFinishedMsg = { finished: boolean };
type WsMsg = WsJsonPatchMsg | WsFinishedMsg;

const decodePointerSegment = (value: string) =>
  value.replace(/~1/g, '/').replace(/~0/g, '~');

export interface UseProjectTasksResult {
  tasks: TaskWithAttemptStatus[];
  tasksById: Record<string, TaskWithAttemptStatus>;
  tasksByStatus: Record<TaskStatus, TaskWithAttemptStatus[]>;
  sharedTasksById: Record<string, SharedTaskRecord>;
  sharedOnlyByStatus: Record<TaskStatus, SharedTaskRecord[]>;
  isLoading: boolean;
  /** True once initial fetch AND local state sync are complete */
  isInitialSyncComplete: boolean;
  error: string | null;
  // Per-status pagination controls
  loadMoreByStatus: Record<TaskStatus, () => void>;
  isLoadingMoreByStatus: Record<TaskStatus, boolean>;
  hasMoreByStatus: Record<TaskStatus, boolean>;
  totalByStatus: Record<TaskStatus, number>;
}

export const useProjectTasks = (projectId: string): UseProjectTasksResult => {
  const queryClient = useQueryClient();
  const { project } = useProject();
  const { isSignedIn } = useAuth();
  const remoteProjectId = project?.remote_project_id;
  const [tasksById, setTasksById] = useState<TasksState['tasks']>({});
  const [paginationByStatus, setPaginationByStatus] = useState<PerStatusPagination>(createInitialPaginationState);
  const [error, setError] = useState<string | null>(null);

  const mergeTasks = useCallback((newTasks: TaskWithAttemptStatus[], replace: boolean) => {
    setTasksById((prev) => {
      if (replace) {
        const map: Record<string, TaskWithAttemptStatus> = {};
        for (const task of newTasks) {
          map[task.id] = task;
        }
        return map;
      }
      const next = { ...prev };
      for (const task of newTasks) {
        next[task.id] = task;
      }
      return next;
    });
  }, []);

  // Memoize query configurations to prevent recreation on every render
  const queryConfigs = useMemo(
    () =>
      ALL_STATUSES.map((status) => ({
        queryKey: projectTasksKeys.byProjectAndStatus(projectId || undefined, status),
        queryFn: () =>
          tasksApi.list(projectId, {
            offset: 0,
            limit: PAGE_SIZE,
            status,
            order_by: getOrderByForStatus(status),
          }).then((page) => ({ status, page })),
        enabled: !!projectId,
        staleTime: 30_000, // 30 seconds - prevents refetches on component remounts
        refetchOnMount: false as const,
        refetchOnWindowFocus: false as const,
      })),
    [projectId]
  );

  // Initial fetch - load all statuses in parallel using React Query for automatic deduplication
  const statusQueries = useQueries({ queries: queryConfigs });

  // Track which query data we've already synced to avoid re-syncing
  const [syncedDataIds, setSyncedDataIds] = useState<string | null>(null);

  // Extract stable primitive values from queries to avoid dependency on the array reference
  const allQueriesSuccess = statusQueries.every((q) => q.isSuccess);
  const anyQueryError = statusQueries.some((q) => q.isError);
  // Extract error message as a stable string primitive
  const firstErrorMessage = useMemo(() => {
    if (!anyQueryError) return null;
    const errorObj = statusQueries.find((q) => q.isError)?.error;
    return errorObj instanceof Error ? errorObj.message : 'Failed to load tasks';
  }, [anyQueryError, statusQueries]);

  // Compute a stable ID for the current query data - only when all queries succeed
  const queryDataId = useMemo(() => {
    if (!projectId || !allQueriesSuccess) return null;
    // Create a stable ID based on the data content
    return statusQueries
      .map((q) => q.data?.page.tasks.map((t) => t.id).join(',') ?? '')
      .join('|');
  }, [projectId, allQueriesSuccess, statusQueries]);

  // Sync query results to local state only when data actually changes
  useEffect(() => {
    if (!projectId) {
      setTasksById({});
      setPaginationByStatus(createInitialPaginationState());
      setSyncedDataIds(null);
      return;
    }

    if (anyQueryError && firstErrorMessage) {
      setError(firstErrorMessage);
      return;
    }

    // Only sync if we have new data that hasn't been synced yet
    if (queryDataId === null || queryDataId === syncedDataIds) return;

    // All queries succeeded - merge results to local state
    const allTasks: TaskWithAttemptStatus[] = [];
    const newPagination = createInitialPaginationState();

    for (const query of statusQueries) {
      if (!query.data) continue;
      const { status, page } = query.data;
      allTasks.push(...page.tasks);
      newPagination[status] = {
        offset: page.tasks.length,
        total: page.total,
        hasMore: page.hasMore,
        isLoading: false,
      };
    }

    mergeTasks(allTasks, true);
    setPaginationByStatus(newPagination);
    setError(null);
    setSyncedDataIds(queryDataId);
  }, [projectId, queryDataId, syncedDataIds, anyQueryError, firstErrorMessage, mergeTasks, statusQueries]);

  // Derive loading state from queries
  // In React Query v5, isLoading = isPending && isFetching
  // We want "loading" to mean: we have a projectId but not all queries have completed yet
  const isQueriesLoading = !!projectId && !statusQueries.every((q) => q.isSuccess || q.isError);

  // Initial sync is complete once queries have loaded AND data has been synced to local state
  const isInitialSyncComplete = !isQueriesLoading && syncedDataIds !== null;

  const loadMoreForStatus = useCallback((status: TaskStatus) => {
    const statusPagination = paginationByStatus[status];
    if (!projectId || isQueriesLoading || statusPagination.isLoading || !statusPagination.hasMore) {
      return;
    }

    // Set loading state for this status
    setPaginationByStatus((prev) => ({
      ...prev,
      [status]: { ...prev[status], isLoading: true },
    }));
    setError(null);

    tasksApi
      .list(projectId, {
        offset: statusPagination.offset,
        limit: PAGE_SIZE,
        status,
        order_by: getOrderByForStatus(status),
      })
      .then((page) => {
        mergeTasks(page.tasks, false);
        setPaginationByStatus((prev) => ({
          ...prev,
          [status]: {
            offset: prev[status].offset + page.tasks.length,
            total: page.total,
            hasMore: page.hasMore,
            isLoading: false,
          },
        }));
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : 'Failed to load tasks');
        setPaginationByStatus((prev) => ({
          ...prev,
          [status]: { ...prev[status], isLoading: false },
        }));
      });
  }, [projectId, isQueriesLoading, paginationByStatus, mergeTasks]);

  const applyTaskPatches = useCallback(
    (patches: Operation[]) => {
      if (!patches.length) return;

      // Track changes per status for pagination updates
      const statusDeltas: Partial<Record<TaskStatus, { added: number; removed: number }>> = {};

      setTasksById((prev) => {
        let next = prev;

        for (const op of patches) {
          if (!op.path.startsWith(TASK_PATH_PREFIX)) continue;

          const rawId = op.path.slice(TASK_PATH_PREFIX.length);
          const taskId = decodePointerSegment(rawId);
          if (!taskId) continue;

          if (op.op === 'remove') {
            const existingTask = next[taskId];
            if (!existingTask) continue;
            if (next === prev) next = { ...prev };
            // Track removal for the task's status
            const status = existingTask.status;
            if (!statusDeltas[status]) statusDeltas[status] = { added: 0, removed: 0 };
            statusDeltas[status]!.removed += 1;
            delete next[taskId];
            continue;
          }

          if (op.op !== 'add' && op.op !== 'replace') continue;

          const task = op.value as TaskWithAttemptStatus;
          if (!task || typeof task !== 'object' || !task.id) continue;
          if (task.project_id !== projectId) continue;

          const existingTask = next[task.id];
          if (op.op === 'replace' && !existingTask) continue;

          if (next === prev) next = { ...prev };

          // Track status changes
          if (!existingTask) {
            // New task added
            if (!statusDeltas[task.status]) statusDeltas[task.status] = { added: 0, removed: 0 };
            statusDeltas[task.status]!.added += 1;
          } else if (existingTask.status !== task.status) {
            // Task moved between statuses
            if (!statusDeltas[existingTask.status]) statusDeltas[existingTask.status] = { added: 0, removed: 0 };
            statusDeltas[existingTask.status]!.removed += 1;
            if (!statusDeltas[task.status]) statusDeltas[task.status] = { added: 0, removed: 0 };
            statusDeltas[task.status]!.added += 1;
          }

          next[task.id] = task;
        }

        return next;
      });

      // Update per-status pagination
      const affectedStatuses = Object.keys(statusDeltas) as TaskStatus[];
      if (affectedStatuses.length > 0) {
        setPaginationByStatus((prev) => {
          const next = { ...prev };
          for (const status of affectedStatuses) {
            const delta = statusDeltas[status]!;
            const netChange = delta.added - delta.removed;
            next[status] = {
              ...prev[status],
              total: Math.max(0, prev[status].total + netChange),
              offset: Math.max(0, prev[status].offset + netChange),
            };
          }
          return next;
        });
      }

      // Invalidate React Query cache to keep it in sync
      queryClient.invalidateQueries({ queryKey: projectTasksKeys.byProject(projectId) });
    },
    [projectId, queryClient]
  );

  useEffect(() => {
    if (!projectId) {
      return;
    }

    let ws: WebSocket | null = null;
    let retryTimer: number | null = null;
    let retryAttempts = 0;
    let closed = false;

    const scheduleReconnect = () => {
      if (retryTimer) return;
      const delay = Math.min(8000, 1000 * Math.pow(2, retryAttempts));
      retryTimer = window.setTimeout(() => {
        retryTimer = null;
        connect();
      }, delay);
    };

    const connect = () => {
      if (closed) return;
      const endpoint = `/api/tasks/stream/ws?project_id=${encodeURIComponent(
        projectId
      )}&include_snapshot=false`;
      // Prepend base URL for Tauri (where server runs on dynamic port)
      const fullEndpoint = getApiBaseUrlSync() + endpoint;
      const wsEndpoint = fullEndpoint.replace(/^http/, 'ws');
      ws = new WebSocket(wsEndpoint);

      ws.onopen = () => {
        retryAttempts = 0;
      };

      ws.onmessage = (event) => {
        try {
          const msg: WsMsg = JSON.parse(event.data);
          if ('JsonPatch' in msg) {
            applyTaskPatches(msg.JsonPatch);
          }
          if ('finished' in msg) {
            ws?.close(1000, 'finished');
          }
        } catch (err) {
          console.error('Failed to process task updates stream:', err);
        }
      };

      ws.onerror = () => {
        // Best-effort live updates; ignore errors and rely on reconnects.
      };

      ws.onclose = (evt) => {
        if (closed) return;
        if (evt?.code === 1000 && evt?.wasClean) {
          return;
        }
        retryAttempts += 1;
        scheduleReconnect();
      };
    };

    connect();

    return () => {
      closed = true;
      if (retryTimer) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
      if (ws) {
        ws.onopen = null;
        ws.onmessage = null;
        ws.onerror = null;
        ws.onclose = null;
        ws.close();
        ws = null;
      }
    };
  }, [projectId, applyTaskPatches]);

  const sharedTasksQuery = useLiveQuery(
    useCallback(
      (q) => {
        if (!remoteProjectId || !isSignedIn) {
          return undefined;
        }
        return q
          .from({ sharedTasks: sharedTasksCollection })
          .where(({ sharedTasks }) =>
            eq(sharedTasks.project_id, remoteProjectId)
          )
          .where(({ sharedTasks }) => isNull(sharedTasks.deleted_at));
      },
      [remoteProjectId, isSignedIn]
    ),
    [remoteProjectId, isSignedIn]
  );

  const sharedTasksList = useMemo(
    () => sharedTasksQuery.data ?? [],
    [sharedTasksQuery.data]
  );

  const referencedSharedIds = useMemo(
    () =>
      new Set(
        Object.values(tasksById)
          .map((task) => task.shared_task_id)
          .filter((id): id is string => Boolean(id))
      ),
    [tasksById]
  );

  const { assignees } = useAssigneeUserNames({
    projectId: remoteProjectId || undefined,
    sharedTasks: sharedTasksList,
  });

  const sharedTasksById = useMemo(() => {
    if (!sharedTasksList) return {};
    const map: Record<string, SharedTaskRecord> = {};
    const list = Array.isArray(sharedTasksList) ? sharedTasksList : [];
    for (const task of list) {
      const assignee =
        task.assignee_user_id && assignees
          ? assignees.find((a) => a.user_id === task.assignee_user_id)
          : null;
      map[task.id] = {
        ...task,
        status: task.status,
        remote_project_id: task.project_id,
        assignee_first_name: assignee?.first_name ?? null,
        assignee_last_name: assignee?.last_name ?? null,
        assignee_username: assignee?.username ?? null,
      };
    }
    return map;
  }, [sharedTasksList, assignees]);

  const { tasks, tasksByStatus } = useMemo(() => {
    const merged: Record<string, TaskWithAttemptStatus> = { ...tasksById };
    const byStatus: Record<TaskStatus, TaskWithAttemptStatus[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    Object.values(merged).forEach((task) => {
      byStatus[task.status]?.push(task);
    });

    // Sort each status list according to its order_by preference
    for (const status of ALL_STATUSES) {
      sortTasksForStatus(byStatus[status], status);
    }

    // Flat list sorted by created_at desc (most recent first)
    const sorted = Object.values(merged).sort(
      (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
    );

    return { tasks: sorted, tasksByStatus: byStatus };
  }, [tasksById]);

  const sharedOnlyByStatus = useMemo(() => {
    const grouped: Record<TaskStatus, SharedTaskRecord[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    Object.values(sharedTasksById).forEach((sharedTask) => {
      const hasLocal =
        Boolean(tasksById[sharedTask.id]) ||
        referencedSharedIds.has(sharedTask.id);

      if (hasLocal) {
        return;
      }
      grouped[sharedTask.status]?.push(sharedTask);
    });

    (Object.values(grouped) as SharedTaskRecord[][]).forEach((list) => {
      list.sort(
        (a, b) =>
          new Date(b.created_at as string).getTime() -
          new Date(a.created_at as string).getTime()
      );
    });

    return grouped;
  }, [tasksById, sharedTasksById, referencedSharedIds]);

  // Derive per-status pagination helpers for the return value
  const loadMoreByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = () => loadMoreForStatus(status);
        return acc;
      },
      {} as Record<TaskStatus, () => void>
    );
  }, [loadMoreForStatus]);

  const isLoadingMoreByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = paginationByStatus[status].isLoading;
        return acc;
      },
      {} as Record<TaskStatus, boolean>
    );
  }, [paginationByStatus]);

  const hasMoreByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = paginationByStatus[status].hasMore;
        return acc;
      },
      {} as Record<TaskStatus, boolean>
    );
  }, [paginationByStatus]);

  const totalByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = paginationByStatus[status].total;
        return acc;
      },
      {} as Record<TaskStatus, number>
    );
  }, [paginationByStatus]);

  // For auto-link, we want to check if ANY status still has more to load
  const anyStatusHasMore = useMemo(
    () => ALL_STATUSES.some((status) => paginationByStatus[status].hasMore),
    [paginationByStatus]
  );

  // Auto-link shared tasks assigned to current user
  useAutoLinkSharedTasks({
    sharedTasksById,
    localTasksById: tasksById,
    referencedSharedIds,
    isLoading: isQueriesLoading,
    hasMore: anyStatusHasMore,
    remoteProjectId: project?.remote_project_id || undefined,
    projectId,
  });

  return {
    tasks,
    tasksById,
    tasksByStatus,
    sharedTasksById,
    sharedOnlyByStatus,
    isLoading: isQueriesLoading,
    isInitialSyncComplete,
    error,
    loadMoreByStatus,
    isLoadingMoreByStatus,
    hasMoreByStatus,
    totalByStatus,
  };
};
