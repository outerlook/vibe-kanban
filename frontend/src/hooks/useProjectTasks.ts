import { useCallback, useEffect, useMemo, useState } from 'react';
import { useAuth } from '@/hooks';
import { useProject } from '@/contexts/ProjectContext';
import { useLiveQuery, eq, isNull } from '@tanstack/react-db';
import { sharedTasksCollection } from '@/lib/electric/sharedTasksCollection';
import { useAssigneeUserNames } from './useAssigneeUserName';
import { useAutoLinkSharedTasks } from './useAutoLinkSharedTasks';
import { tasksApi } from '@/lib/api';
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

type TasksState = {
  tasks: Record<string, TaskWithAttemptStatus>;
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
  const [tasksById, setTasksById] = useState<TasksState['tasks']>({});
  const [offset, setOffset] = useState(0);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mergeTasks = useCallback(
    (incoming: TaskWithAttemptStatus[], replace: boolean) => {
      setTasksById((prev) => {
        const next = replace ? {} : { ...prev };
        incoming.forEach((task) => {
          next[task.id] = task;
        });
        return next;
      });
    },
    []
  );

  useEffect(() => {
    if (!projectId) {
      setTasksById({});
      setOffset(0);
      setTotal(0);
      setHasMore(false);
      setIsLoading(false);
      setIsLoadingMore(false);
      setError(null);
      return;
    }

    let cancelled = false;
    setIsLoading(true);
    setIsLoadingMore(false);
    setError(null);
    setOffset(0);
    setTotal(0);
    setHasMore(false);

    tasksApi
      .list(projectId, { offset: 0, limit: PAGE_SIZE })
      .then((page) => {
        if (cancelled) return;
        mergeTasks(page.tasks, true);
        setOffset(page.tasks.length);
        setTotal(page.total);
        setHasMore(page.hasMore);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : 'Failed to load tasks');
      })
      .finally(() => {
        if (cancelled) return;
        setIsLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [projectId, mergeTasks]);

  const loadMore = useCallback(() => {
    if (!projectId || isLoading || isLoadingMore || !hasMore) {
      return;
    }

    setIsLoadingMore(true);
    setError(null);

    tasksApi
      .list(projectId, { offset, limit: PAGE_SIZE })
      .then((page) => {
        mergeTasks(page.tasks, false);
        setOffset(offset + page.tasks.length);
        setTotal(page.total);
        setHasMore(page.hasMore);
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : 'Failed to load tasks');
      })
      .finally(() => {
        setIsLoadingMore(false);
      });
  }, [projectId, isLoading, isLoadingMore, hasMore, offset, mergeTasks]);

  const applyTaskPatches = useCallback(
    (patches: Operation[]) => {
      if (!patches.length) return;

      setTasksById((prev) => {
        let next = prev;
        let added = 0;
        let removed = 0;

        for (const op of patches) {
          if (!op.path.startsWith(TASK_PATH_PREFIX)) continue;

          const rawId = op.path.slice(TASK_PATH_PREFIX.length);
          const taskId = decodePointerSegment(rawId);
          if (!taskId) continue;

          if (op.op === 'remove') {
            if (!next[taskId]) continue;
            if (next === prev) next = { ...prev };
            delete next[taskId];
            removed += 1;
            continue;
          }

          if (op.op !== 'add' && op.op !== 'replace') continue;

          const value = (op as { value?: unknown }).value;
          if (!value) continue;

          const task = value as TaskWithAttemptStatus;
          if (task.project_id !== projectId) continue;

          const exists = Boolean(next[task.id]);
          if (op.op === 'replace' && !exists) continue;

          if (next === prev) next = { ...prev };
          if (!exists) added += 1;
          next[task.id] = task;
        }

        if (added || removed) {
          const delta = added - removed;
          setTotal((current) => Math.max(0, current + delta));
          setOffset((current) => Math.max(0, current + delta));
        }

        return next;
      });
    },
    [projectId]
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

  const localTasksById = useMemo(() => tasksById, [tasksById]);

  const referencedSharedIds = useMemo(
    () =>
      new Set(
        Object.values(localTasksById)
          .map((task) => task.shared_task_id)
          .filter((id): id is string => Boolean(id))
      ),
    [localTasksById]
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

  const { tasks, tasksById: mergedTasksById, tasksByStatus } = useMemo(() => {
    const merged: Record<string, TaskWithAttemptStatus> = { ...localTasksById };
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

    const sorted = Object.values(merged).sort(
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

    return { tasks: sorted, tasksById: merged, tasksByStatus: byStatus };
  }, [localTasksById]);

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
        Boolean(localTasksById[sharedTask.id]) ||
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
  }, [localTasksById, sharedTasksById, referencedSharedIds]);

  // Auto-link shared tasks assigned to current user
  useAutoLinkSharedTasks({
    sharedTasksById,
    localTasksById,
    referencedSharedIds,
    isLoading,
    hasMore,
    remoteProjectId: project?.remote_project_id || undefined,
    projectId,
  });

  return {
    tasks,
    tasksById: mergedTasksById,
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
