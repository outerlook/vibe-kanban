import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  transformToSvarFormat,
  type SvarGanttTask,
  type SvarGanttLink,
  type TransformOptions,
} from '@/lib/transformGantt';
import { ganttApi, getApiBaseUrlSync } from '@/lib/api';
import type { GanttTask } from 'shared/types';
import type { Operation } from 'rfc6902';

const PAGE_SIZE = 50;
const GANTT_TASK_PATH_PREFIX = '/gantt_tasks/';

type WsJsonPatchMsg = { JsonPatch: Operation[] };
type WsFinishedMsg = { finished: boolean };
type WsMsg = WsJsonPatchMsg | WsFinishedMsg;

const decodePointerSegment = (value: string) =>
  value.replace(/~1/g, '/').replace(/~0/g, '~');

export interface UseGanttTasksOptions {
  colorMode?: TransformOptions['colorMode'];
}

export interface UseGanttTasksResult {
  ganttTasks: SvarGanttTask[];
  ganttLinks: SvarGanttLink[];
  isLoading: boolean;
  isLoadingMore: boolean;
  total: number;
  hasMore: boolean;
  loadMore: () => void;
  error: string | null;
}

/**
 * Hook that fetches Gantt data via REST with pagination and subscribes to
 * real-time updates via WebSocket (only for already-loaded tasks).
 */
export const useGanttTasks = (
  projectId: string | undefined,
  options: UseGanttTasksOptions = {}
): UseGanttTasksResult => {
  const { colorMode = 'status' } = options;
  const [tasksById, setTasksById] = useState<Record<string, GanttTask>>({});
  const [offset, setOffset] = useState(0);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mergeTasks = useCallback(
    (incoming: GanttTask[], replace: boolean) => {
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

  // Initial load when projectId changes
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

    ganttApi
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
        setError(
          err instanceof Error ? err.message : 'Failed to load gantt tasks'
        );
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

    ganttApi
      .list(projectId, { offset, limit: PAGE_SIZE })
      .then((page) => {
        mergeTasks(page.tasks, false);
        setOffset(offset + page.tasks.length);
        setTotal(page.total);
        setHasMore(page.hasMore);
      })
      .catch((err) => {
        setError(
          err instanceof Error ? err.message : 'Failed to load gantt tasks'
        );
      })
      .finally(() => {
        setIsLoadingMore(false);
      });
  }, [projectId, isLoading, isLoadingMore, hasMore, offset, mergeTasks]);

  const applyGanttPatches = useCallback(
    (patches: Operation[]) => {
      if (!patches.length) return;

      setTasksById((prev) => {
        let next = prev;

        for (const op of patches) {
          if (!op.path.startsWith(GANTT_TASK_PATH_PREFIX)) continue;

          const rawId = op.path.slice(GANTT_TASK_PATH_PREFIX.length);
          const taskId = decodePointerSegment(rawId);
          if (!taskId) continue;

          if (op.op === 'remove') {
            // Only remove if task exists in our loaded set
            if (!next[taskId]) continue;
            if (next === prev) next = { ...prev };
            delete next[taskId];
            continue;
          }

          // For add/replace, only apply if task already exists (replace)
          // or if it's a new task we should ignore (add for tasks outside range)
          if (op.op !== 'add' && op.op !== 'replace') continue;

          const value = (op as { value?: unknown }).value;
          if (!value) continue;

          const task = value as GanttTask;

          // For 'replace', only update if already loaded
          if (op.op === 'replace') {
            if (!next[task.id]) continue;
            if (next === prev) next = { ...prev };
            next[task.id] = task;
          }

          // For 'add', ignore new tasks outside loaded range
          // They will appear when user loads more or refreshes
        }

        return next;
      });
    },
    []
  );

  // WebSocket connection for real-time updates
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
      const endpoint = `/api/projects/${encodeURIComponent(projectId)}/gantt/stream/ws`;
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
            applyGanttPatches(msg.JsonPatch);
          }
          if ('finished' in msg) {
            ws?.close(1000, 'finished');
          }
        } catch (err) {
          console.error('Failed to process gantt updates stream:', err);
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
  }, [projectId, applyGanttPatches]);

  const { tasks: ganttTasks, links: ganttLinks } = useMemo(
    () => transformToSvarFormat(tasksById, { colorMode }),
    [tasksById, colorMode]
  );

  return {
    ganttTasks,
    ganttLinks,
    isLoading,
    isLoadingMore,
    total,
    hasMore,
    loadMore,
    error,
  };
};
