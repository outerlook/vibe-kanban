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
import { useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { useProject } from '@/contexts/ProjectContext';
import { tasksApi, getApiBaseUrlSync } from '@/lib/api';
import {
  projectTasksKeys,
  setTaskInCache,
  removeTaskFromCache,
  moveTaskBetweenStatuses,
  type StatusQueryData,
} from '@/lib/taskCacheHelpers';
import type { Operation } from 'rfc6902';
import type { HookExecution, OperationStatus, TaskStatus, TaskWithAttemptStatus } from 'shared/types';

// Type for operation statuses stored in React Query cache
type OperationStatusesData = Record<string, OperationStatus>;

const operationStatusesKey = ['operationStatuses'] as const;


const PAGE_SIZE = 25;
const ALL_STATUSES: TaskStatus[] = ['todo', 'inprogress', 'inreview', 'done', 'cancelled'];
const TASK_PATH_PREFIX = '/tasks/';
const OPERATION_STATUS_PATH_PREFIX = '/operation_status/';
const HOOK_EXECUTION_PATH_PREFIX = '/hook_executions/';

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
  hookExecutionsByTaskId: Record<string, HookExecution[]>;
  paginationByStatus: PerStatusPagination;
  isQueriesLoading: boolean;
  isInitialSyncComplete: boolean;
  error: string | null;
  loadMoreForStatus: (status: TaskStatus) => void;
}

const ProjectTasksContext = createContext<ProjectTasksContextValue | null>(null);

interface ProjectTasksProviderProps {
  children: ReactNode;
}

export function ProjectTasksProvider({ children }: ProjectTasksProviderProps) {
  const { projectId } = useProject();
  const queryClient = useQueryClient();
  const [hookExecutionsByTaskId, setHookExecutionsByTaskId] = useState<Record<string, HookExecution[]>>({});
  const [paginationByStatus, setPaginationByStatus] = useState<PerStatusPagination>(createInitialPaginationState);
  const [error, setError] = useState<string | null>(null);

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

  // Derive tasksById from React Query cache - single source of truth
  const tasksById = useMemo(() => {
    const result: Record<string, TaskWithAttemptStatus> = {};
    for (const query of statusQueries) {
      if (!query.data) continue;
      for (const task of query.data.page.tasks) {
        result[task.id] = task;
      }
    }
    return result;
  }, [statusQueries]);

  // Reset hook executions when project changes
  useEffect(() => {
    if (!projectId) {
      setHookExecutionsByTaskId({});
    }
  }, [projectId]);

  // Sync pagination state from query results when queries complete
  useEffect(() => {
    if (!projectId) {
      setPaginationByStatus(createInitialPaginationState);
      return;
    }
    if (allQueriesSuccess) {
      const queries = statusQueriesRef.current;
      setPaginationByStatus((prev) => {
        // Derive pagination from current query results
        const derived = createInitialPaginationState();
        for (const query of queries) {
          if (!query.data) continue;
          const { status, page } = query.data;
          derived[status] = {
            offset: page.tasks.length,
            total: page.total,
            hasMore: page.hasMore,
            isLoading: false,
          };
        }

        // Only update if there's a meaningful difference
        const hasChanges = ALL_STATUSES.some((status) => {
          const current = prev[status];
          return (
            derived[status].total !== current.total ||
            derived[status].hasMore !== current.hasMore ||
            // Only sync offset if not currently loading more
            (!current.isLoading && derived[status].offset !== current.offset)
          );
        });
        return hasChanges ? derived : prev;
      });
    }
  }, [projectId, allQueriesSuccess]);

  // Set error from query failures
  useEffect(() => {
    if (anyQueryError && firstErrorMessage) {
      setError(firstErrorMessage);
    } else if (allQueriesSuccess) {
      setError(null);
    }
  }, [anyQueryError, firstErrorMessage, allQueriesSuccess]);

  const isQueriesLoading = !!projectId && !statusQueries.every((q) => q.isSuccess || q.isError);
  const isInitialSyncComplete = allQueriesSuccess;

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
        // Add new tasks to React Query cache
        const queryKey = projectTasksKeys.byProjectAndStatus(projectId, status);
        queryClient.setQueryData<StatusQueryData>(queryKey, (oldData) => {
          if (!oldData) return oldData;
          // Merge new tasks, avoiding duplicates
          const existingIds = new Set(oldData.page.tasks.map((t) => t.id));
          const newTasks = page.tasks.filter((t) => !existingIds.has(t.id));
          return {
            ...oldData,
            page: {
              tasks: [...oldData.page.tasks, ...newTasks],
              total: page.total,
              hasMore: page.hasMore,
            },
          };
        });
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
  }, [projectId, isQueriesLoading, paginationByStatus, queryClient]);

  // Helper to find task's current status from React Query cache
  const findTaskStatusInCache = useCallback(
    (taskId: string): TaskStatus | null => {
      if (!projectId) return null;
      for (const status of ALL_STATUSES) {
        const queryKey = projectTasksKeys.byProjectAndStatus(projectId, status);
        const data = queryClient.getQueryData<StatusQueryData>(queryKey);
        if (data?.page.tasks.some((t) => t.id === taskId)) {
          return status;
        }
      }
      return null;
    },
    [projectId, queryClient]
  );

  // Apply patches from WebSocket - updates React Query cache directly
  const applyTaskPatches = useCallback(
    (patches: Operation[]) => {
      if (!patches.length || !projectId) return;

      const statusDeltas: Partial<Record<TaskStatus, { added: number; removed: number }>> = {};

      for (const op of patches) {
        if (!op.path.startsWith(TASK_PATH_PREFIX)) continue;

        const rawId = op.path.slice(TASK_PATH_PREFIX.length);
        const taskId = decodePointerSegment(rawId);
        if (!taskId) continue;

        if (op.op === 'remove') {
          // Find the task's current status in cache to remove it
          const existingStatus = findTaskStatusInCache(taskId);
          if (!existingStatus) continue;

          removeTaskFromCache(queryClient, taskId, projectId, existingStatus);

          // Track delta for pagination state
          if (!statusDeltas[existingStatus]) statusDeltas[existingStatus] = { added: 0, removed: 0 };
          statusDeltas[existingStatus]!.removed += 1;
          continue;
        }

        if (op.op !== 'add' && op.op !== 'replace') continue;

        const task = op.value as TaskWithAttemptStatus;
        if (!task || typeof task !== 'object' || !task.id) continue;
        if (task.project_id !== projectId) continue;

        // Find existing task's status to detect status changes
        const existingStatus = findTaskStatusInCache(task.id);

        if (op.op === 'replace' && !existingStatus) continue;

        if (!existingStatus) {
          // New task - add to cache
          setTaskInCache(queryClient, task, projectId);

          // Track delta for pagination state
          if (!statusDeltas[task.status]) statusDeltas[task.status] = { added: 0, removed: 0 };
          statusDeltas[task.status]!.added += 1;
        } else if (existingStatus !== task.status) {
          // Status changed - move between status lists
          moveTaskBetweenStatuses(queryClient, task, existingStatus, task.status, projectId);

          // Track deltas for pagination state
          if (!statusDeltas[existingStatus]) statusDeltas[existingStatus] = { added: 0, removed: 0 };
          statusDeltas[existingStatus]!.removed += 1;
          if (!statusDeltas[task.status]) statusDeltas[task.status] = { added: 0, removed: 0 };
          statusDeltas[task.status]!.added += 1;
        } else {
          // Same status - just update in place
          setTaskInCache(queryClient, task, projectId);
        }
      }

      // Update pagination state for affected statuses
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
    [projectId, queryClient, findTaskStatusInCache]
  );

  // Apply operation status patches from WebSocket - updates React Query cache directly
  const applyOperationStatusPatches = useCallback((patches: Operation[]) => {
    if (!patches.length) return;

    queryClient.setQueryData<OperationStatusesData>(operationStatusesKey, (prev) => {
      let next = prev ?? {};

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
  }, [queryClient]);

  // Apply hook execution patches from WebSocket
  // Path format: /hook_executions/{task_id}/{execution_id}
  const applyHookExecutionPatches = useCallback((patches: Operation[]) => {
    if (!patches.length) return;

    setHookExecutionsByTaskId((prev) => {
      let next = prev;

      for (const op of patches) {
        if (!op.path.startsWith(HOOK_EXECUTION_PATH_PREFIX)) continue;

        const remainder = op.path.slice(HOOK_EXECUTION_PATH_PREFIX.length);
        const slashIndex = remainder.indexOf('/');
        if (slashIndex === -1) continue;

        const taskId = decodePointerSegment(remainder.slice(0, slashIndex));
        const executionId = decodePointerSegment(remainder.slice(slashIndex + 1));
        if (!taskId || !executionId) continue;

        if (op.op === 'remove') {
          const existingList = next[taskId];
          if (!existingList) continue;
          const idx = existingList.findIndex((e) => e.id === executionId);
          if (idx === -1) continue;
          if (next === prev) next = { ...prev };
          const newList = existingList.filter((e) => e.id !== executionId);
          if (newList.length === 0) {
            delete next[taskId];
          } else {
            next[taskId] = newList;
          }
          continue;
        }

        if (op.op !== 'add' && op.op !== 'replace') continue;

        const execution = op.value as HookExecution;
        if (!execution || typeof execution !== 'object' || !execution.id) continue;

        if (next === prev) next = { ...prev };

        const existingList = next[taskId] ?? [];
        const idx = existingList.findIndex((e) => e.id === executionId);

        if (op.op === 'add') {
          if (idx === -1) {
            next[taskId] = [...existingList, execution];
          }
        } else {
          // replace
          if (idx !== -1) {
            const newList = [...existingList];
            newList[idx] = execution;
            next[taskId] = newList;
          }
        }
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
      )}&include_snapshot=true`;
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
            applyHookExecutionPatches(msg.JsonPatch);
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
  }, [projectId, applyTaskPatches, applyOperationStatusPatches, applyHookExecutionPatches]);

  // Subscribe to operationStatuses from React Query cache
  // This query has no queryFn - it's only updated via setQueryData from WebSocket patches
  const { data: operationStatuses = {} } = useQuery<OperationStatusesData>({
    queryKey: operationStatusesKey,
    queryFn: () => ({}),
    staleTime: Infinity,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
  });

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
      hookExecutionsByTaskId,
      paginationByStatus,
      isQueriesLoading,
      isInitialSyncComplete,
      error,
      loadMoreForStatus,
    }),
    [tasksById, operationStatuses, operationStatusesByTaskId, hookExecutionsByTaskId, paginationByStatus, isQueriesLoading, isInitialSyncComplete, error, loadMoreForStatus]
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
