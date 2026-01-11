import { useCallback, useMemo } from 'react';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import {
  transformToFrappeFormat,
  type FrappeGanttTask,
} from '@/lib/transformGantt';
import type {
  GanttTask,
  GanttExecutionOverlay,
} from 'shared/types';

/**
 * State shape for Gantt data received from the WebSocket stream.
 * Field names must match the JSON patch paths sent by the backend.
 */
interface GanttStreamState {
  gantt_tasks: Record<string, GanttTask>;
  executions: Record<string, GanttExecutionOverlay>;
}

export interface UseGanttTasksResult {
  /** Frappe-gantt-ready task array */
  ganttTasks: FrappeGanttTask[];
  /** Raw tasks by ID */
  tasksById: Record<string, GanttTask>;
  /** Execution overlays by task_id */
  executions: Record<string, GanttExecutionOverlay>;
  /** Whether initial data is loading */
  isLoading: boolean;
  /** Whether WebSocket is connected */
  isConnected: boolean;
  /** Error message if any */
  error: string | null;
}

/**
 * Hook that fetches Gantt data and subscribes to real-time updates via WebSocket.
 * Falls back gracefully when WebSocket is unavailable.
 */
export const useGanttTasks = (projectId: string | undefined): UseGanttTasksResult => {
  const endpoint = projectId
    ? `/api/projects/${encodeURIComponent(projectId)}/gantt/stream/ws`
    : undefined;

  const initialData = useCallback(
    (): GanttStreamState => ({
      gantt_tasks: {},
      executions: {},
    }),
    []
  );

  const { data, isConnected, error } = useJsonPatchWsStream<GanttStreamState>(
    endpoint,
    Boolean(projectId),
    initialData
  );

  const tasksById = data?.gantt_tasks ?? {};
  const executions = data?.executions ?? {};

  // Transform to frappe-gantt format, memoized to avoid re-renders
  const ganttTasks = useMemo(
    () => transformToFrappeFormat(tasksById),
    [tasksById]
  );

  // Consider loading if we have a projectId but no data yet and no error
  const isLoading = Boolean(projectId) && !data && !error;

  return {
    ganttTasks,
    tasksById,
    executions,
    isLoading,
    isConnected,
    error,
  };
};
