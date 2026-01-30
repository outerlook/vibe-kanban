import {
  createContext,
  useContext,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { useQueries } from '@tanstack/react-query';
import { useProject } from '@/contexts/ProjectContext';
import { tasksApi, getApiBaseUrlSync } from '@/lib/api';
import { projectTasksKeys } from '@/lib/taskCacheHelpers';
import type { Operation } from 'rfc6902';
import type { OperationStatus, TaskStatus, TaskWithAttemptStatus } from 'shared/types';

// Re-export for backwards compatibility
export { projectTasksKeys };

const PAGE_SIZE = 25;
const ALL_STATUSES: TaskStatus[] = ['todo', 'inprogress', 'inreview', 'done', 'cancelled'];
const TASK_PATH_PREFIX = '/tasks/';
const OPERATION_STATUS_PATH_PREFIX = '/operation_status/';

type WsJsonPatchMsg = { JsonPatch: Operation[] };
type WsFinishedMsg = { finished: boolean };
type WsMsg = WsJsonPatchMsg | WsFinishedMsg;

type StatusPaginationState = {
  offset: number;
  total: number;
  hasMore: boolean;
  isLoading: boolean;
};

type PerStatusPagination = Record<TaskStatus, StatusPaginationState>;

const getOrderByForStatus = (status: TaskStatus): 'created_at_asc' | 'updated_at_desc' => {
  return status === 'done' || status === 'cancelled' ? 'updated_at_desc' : 'created_at_asc';
};

const createInitialPaginationState = (): PerStatusPagination => ({
  todo: { offset: 0, total: 0, hasMore: false, isLoading: false },
  inprogress: { offset: 0, total: 0, hasMore: false, isLoading: false },
  inreview: { offset: 0, total: 0, hasMore: false, isLoading: false },
  done: { offset: 0, total: 0, hasMore: false, isLoading: false },
  cancelled: { offset: 0, total: 0, hasMore: false, isLoading: false },
});

const decodePointerSegment = (value: string) =>
  value.replace(/~1/g, '/').replace(/~0/g, '~');

interface ProjectTasksContextValue {
  tasksById: Record<string, TaskWithAttemptStatus>;
  operationStatuses: Record<string, OperationStatus>;
  operationStatusesByTaskId: Record<string, OperationStatus>;
  paginationByStatus: PerStatusPagination;
  isQueriesLoading: boolean;
  isInitialSyncComplete: boolean;
  error: string | null;
  loadMoreForStatus: (status: TaskStatus) => void;
  mergeTasks: (tasks: TaskWithAttemptStatus[], replace: boolean) => void;
}

const ProjectTasksContext = createContext<ProjectTasksContextValue | null>(null);

interface ProjectTasksProviderProps {
  children: ReactNode;
}

export function ProjectTasksProvider({ children }: ProjectTasksProviderProps) {
  const { projectId } = useProject();
  const [tasksById, setTasksById] = useState<Record<string, TaskWithAttemptStatus>>({});
  const [operationStatuses, setOperationStatuses] = useState<Record<string, OperationStatus>>({});
  const [paginationByStatus, setPaginationByStatus] = useState<PerStatusPagination>(createInitialPaginationState);
  const [error, setError] = useState<string | null>(null);
  const [syncedDataIds, setSyncedDataIds] = useState<string | null>(null);

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

  // Memoize query configurations
  const queryConfigs = useMemo(
    () =>
      ALL_STATUSES.map((status) => ({
        queryKey: projectTasksKeys.byProjectAndStatus(projectId || undefined, status),
        queryFn: () =>
          tasksApi.list(projectId!, {
            offset: 0,
            limit: PAGE_SIZE,
            status,
            order_by: getOrderByForStatus(status),
          }).then((page) => ({ status, page })),
        enabled: !!projectId,
        staleTime: 30_000,
        refetchOnMount: false as const,
        refetchOnWindowFocus: false as const,
      })),
    [projectId]
  );

  const statusQueries = useQueries({ queries: queryConfigs });
  const statusQueriesRef = useRef(statusQueries);
  statusQueriesRef.current = statusQueries;

  const allQueriesSuccess = statusQueries.every((q) => q.isSuccess);
  const anyQueryError = statusQueries.some((q) => q.isError);

  const firstErrorMessage = useMemo(() => {
    if (!anyQueryError) return null;
    const errorObj = statusQueries.find((q) => q.isError)?.error;
    return errorObj instanceof Error ? errorObj.message : 'Failed to load tasks';
  }, [anyQueryError, statusQueries]);

  const queryDataId = useMemo(() => {
    if (!projectId || !allQueriesSuccess) return null;
    return statusQueries
      .map((q) => q.data?.page.tasks.map((t) => t.id).join(',') ?? '')
      .join('|');
  }, [projectId, allQueriesSuccess, statusQueries]);

  // Sync query results to local state
  useEffect(() => {
    if (!projectId) {
      setTasksById((prev) => (Object.keys(prev).length === 0 ? prev : {}));
      setOperationStatuses((prev) => (Object.keys(prev).length === 0 ? prev : {}));
      setPaginationByStatus(createInitialPaginationState);
      setSyncedDataIds(null);
      return;
    }

    if (anyQueryError && firstErrorMessage) {
      setError(firstErrorMessage);
      return;
    }

    if (queryDataId === null || queryDataId === syncedDataIds) return;

    const queries = statusQueriesRef.current;
    const allTasks: TaskWithAttemptStatus[] = [];
    const newPagination = createInitialPaginationState();

    for (const query of queries) {
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
  }, [projectId, queryDataId, syncedDataIds, anyQueryError, firstErrorMessage, mergeTasks]);

  const isQueriesLoading = !!projectId && !statusQueries.every((q) => q.isSuccess || q.isError);
  const isInitialSyncComplete = !isQueriesLoading && syncedDataIds !== null;

  const loadMoreForStatus = useCallback((status: TaskStatus) => {
    const statusPagination = paginationByStatus[status];
    if (!projectId || isQueriesLoading || statusPagination.isLoading || !statusPagination.hasMore) {
      return;
    }

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

  // Apply patches from WebSocket
  const applyTaskPatches = useCallback(
    (patches: Operation[]) => {
      if (!patches.length) return;

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

          if (!existingTask) {
            if (!statusDeltas[task.status]) statusDeltas[task.status] = { added: 0, removed: 0 };
            statusDeltas[task.status]!.added += 1;
          } else if (existingTask.status !== task.status) {
            if (!statusDeltas[existingTask.status]) statusDeltas[existingTask.status] = { added: 0, removed: 0 };
            statusDeltas[existingTask.status]!.removed += 1;
            if (!statusDeltas[task.status]) statusDeltas[task.status] = { added: 0, removed: 0 };
            statusDeltas[task.status]!.added += 1;
          }

          next[task.id] = task;
        }

        return next;
      });

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
    },
    [projectId]
  );

  // Apply operation status patches from WebSocket
  const applyOperationStatusPatches = useCallback((patches: Operation[]) => {
    if (!patches.length) return;

    setOperationStatuses((prev) => {
      let next = prev;

      for (const op of patches) {
        if (!op.path.startsWith(OPERATION_STATUS_PATH_PREFIX)) continue;

        const rawId = op.path.slice(OPERATION_STATUS_PATH_PREFIX.length);
        const workspaceId = decodePointerSegment(rawId);
        if (!workspaceId) continue;

        if (op.op === 'remove') {
          if (!next[workspaceId]) continue;
          if (next === prev) next = { ...prev };
          delete next[workspaceId];
          continue;
        }

        if (op.op !== 'add' && op.op !== 'replace') continue;

        const status = op.value as OperationStatus;
        if (!status || typeof status !== 'object' || !status.workspace_id) continue;

        if (op.op === 'replace' && !next[workspaceId]) continue;

        if (next === prev) next = { ...prev };
        next[workspaceId] = status;
      }

      return next;
    });
  }, []);

  // Single WebSocket connection for the entire app
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
            applyOperationStatusPatches(msg.JsonPatch);
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
  }, [projectId, applyTaskPatches, applyOperationStatusPatches]);

  // Compute a mapping from task_id to OperationStatus for quick lookup
  const operationStatusesByTaskId = useMemo(() => {
    const byTaskId: Record<string, OperationStatus> = {};
    for (const status of Object.values(operationStatuses)) {
      byTaskId[status.task_id] = status;
    }
    return byTaskId;
  }, [operationStatuses]);

  const value = useMemo(
    () => ({
      tasksById,
      operationStatuses,
      operationStatusesByTaskId,
      paginationByStatus,
      isQueriesLoading,
      isInitialSyncComplete,
      error,
      loadMoreForStatus,
      mergeTasks,
    }),
    [tasksById, operationStatuses, operationStatusesByTaskId, paginationByStatus, isQueriesLoading, isInitialSyncComplete, error, loadMoreForStatus, mergeTasks]
  );

  return (
    <ProjectTasksContext.Provider value={value}>
      {children}
    </ProjectTasksContext.Provider>
  );
}

export function useProjectTasksContext(): ProjectTasksContextValue {
  const context = useContext(ProjectTasksContext);
  if (!context) {
    throw new Error('useProjectTasksContext must be used within a ProjectTasksProvider');
  }
  return context;
}
