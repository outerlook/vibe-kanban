import { useCallback, useEffect, useMemo } from 'react';
import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query';
import { useAuth } from '@/hooks';
import { useProject } from '@/contexts/ProjectContext';
import { useLiveQuery, eq, isNull } from '@tanstack/react-db';
import { sharedTasksCollection } from '@/lib/electric/sharedTasksCollection';
import { useAssigneeUserNames } from './useAssigneeUserName';
import { useAutoLinkSharedTasks } from './useAutoLinkSharedTasks';
import { tasksApi, type PaginatedTasksResponse } from '@/lib/api';
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
};

export type SharedTaskRecord = SharedTask & {
  remote_project_id: string;
  assignee_first_name?: string | null;
  assignee_last_name?: string | null;
  assignee_username?: string | null;
};

const PAGE_SIZE = 50;
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
  isLoadingMore: boolean;
  total: number;
  hasMore: boolean;
  loadMore: () => void;
  error: string | null;
}

export const useProjectTasks = (projectId: string): UseProjectTasksResult => {
  const { project } = useProject();
  const { isSignedIn } = useAuth();
  const remoteProjectId = project?.remote_project_id;
  const queryClient = useQueryClient();

  const query = useInfiniteQuery({
    queryKey: projectTasksKeys.byProjectInfinite(projectId),
    queryFn: async ({ pageParam = 0 }): Promise<PaginatedTasksResponse> => {
      return tasksApi.list(projectId, { offset: pageParam, limit: PAGE_SIZE });
    },
    getNextPageParam: (lastPage, allPages) =>
      lastPage.hasMore ? allPages.length * PAGE_SIZE : undefined,
    initialPageParam: 0,
    enabled: !!projectId,
  });

  // Derive tasksById from query pages
  const tasksById = useMemo(() => {
    if (!query.data?.pages) return {};
    const map: Record<string, TaskWithAttemptStatus> = {};
    for (const page of query.data.pages) {
      for (const task of page.tasks) {
        map[task.id] = task;
      }
    }
    return map;
  }, [query.data?.pages]);

  // Derive total from the last page (most recent data)
  const total = useMemo(() => {
    if (!query.data?.pages.length) return 0;
    return query.data.pages[query.data.pages.length - 1].total;
  }, [query.data?.pages]);

  const isLoading = query.isLoading;
  const isLoadingMore = query.isFetchingNextPage;
  const hasMore = query.hasNextPage ?? false;
  const error = query.error ? (query.error instanceof Error ? query.error.message : 'Failed to load tasks') : null;

  const loadMore = useCallback(() => {
    if (!query.isFetchingNextPage && query.hasNextPage) {
      query.fetchNextPage();
    }
  }, [query]);

  const applyTaskPatches = useCallback(
    (patches: Operation[]) => {
      if (!patches.length) return;

      queryClient.setQueryData<{ pages: PaginatedTasksResponse[]; pageParams: number[] }>(
        projectTasksKeys.byProjectInfinite(projectId),
        (oldData) => {
          if (!oldData) return oldData;

          // Build a mutable map from all pages
          const taskMap: Record<string, TaskWithAttemptStatus> = {};
          for (const page of oldData.pages) {
            for (const task of page.tasks) {
              taskMap[task.id] = task;
            }
          }

          let added = 0;
          let removed = 0;
          let updated = 0;

          for (const op of patches) {
            if (!op.path.startsWith(TASK_PATH_PREFIX)) continue;

            const rawId = op.path.slice(TASK_PATH_PREFIX.length);
            const taskId = decodePointerSegment(rawId);
            if (!taskId) continue;

            if (op.op === 'remove') {
              if (!taskMap[taskId]) continue;
              delete taskMap[taskId];
              removed += 1;
              continue;
            }

            if (op.op !== 'add' && op.op !== 'replace') continue;

            const value = (op as { value?: unknown }).value;
            if (!value) continue;

            const task = value as TaskWithAttemptStatus;
            if (task.project_id !== projectId) continue;

            const exists = Boolean(taskMap[task.id]);
            if (op.op === 'replace' && !exists) continue;

            if (!exists) added += 1;
            else updated += 1;
            taskMap[task.id] = task;
          }

          if (added === 0 && removed === 0 && updated === 0) return oldData;

          // Rebuild pages with updated tasks
          const allTasks = Object.values(taskMap);
          const delta = added - removed;
          const newTotal = Math.max(0, (oldData.pages[oldData.pages.length - 1]?.total ?? 0) + delta);

          // Redistribute tasks into pages
          const newPages: PaginatedTasksResponse[] = oldData.pages.map((_, idx) => {
            const start = idx * PAGE_SIZE;
            const end = start + PAGE_SIZE;
            const pageTasks = allTasks.slice(start, end);
            return {
              tasks: pageTasks,
              total: newTotal,
              hasMore: end < allTasks.length,
            };
          });

          // Handle case where we have more tasks than pages can hold (new task added)
          const coveredTasks = oldData.pages.length * PAGE_SIZE;
          if (allTasks.length > coveredTasks) {
            // Add remaining tasks to the last page
            const lastPage = newPages[newPages.length - 1];
            if (lastPage) {
              const remainingTasks = allTasks.slice(coveredTasks);
              lastPage.tasks = [...lastPage.tasks, ...remainingTasks];
            }
          }

          return { ...oldData, pages: newPages };
        }
      );
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
      const wsEndpoint = endpoint.replace(/^http/, 'ws');
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
    const byStatus: Record<TaskStatus, TaskWithAttemptStatus[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    Object.values(tasksById).forEach((task) => {
      byStatus[task.status]?.push(task);
    });

    const sorted = Object.values(tasksById).sort(
      (a, b) =>
        new Date(b.created_at as string).getTime() -
        new Date(a.created_at as string).getTime()
    );

    (Object.values(byStatus) as TaskWithAttemptStatus[][]).forEach((list) => {
      list.sort(
        (a, b) =>
          new Date(b.created_at as string).getTime() -
          new Date(a.created_at as string).getTime()
      );
    });

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

  // Auto-link shared tasks assigned to current user
  useAutoLinkSharedTasks({
    sharedTasksById,
    localTasksById: tasksById,
    referencedSharedIds,
    isLoading,
    hasMore,
    remoteProjectId: project?.remote_project_id || undefined,
    projectId,
  });

  return {
    tasks,
    tasksById,
    tasksByStatus,
    sharedTasksById,
    sharedOnlyByStatus,
    isLoading,
    isLoadingMore,
    total,
    hasMore,
    loadMore,
    error,
  };
};
