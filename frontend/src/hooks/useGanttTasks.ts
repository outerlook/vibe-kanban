import { useCallback, useMemo } from 'react';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import {
  transformToFrappeFormat,
  type FrappeGanttTask,
} from '@/lib/transformGantt';
import type { GanttTask } from 'shared/types';

/**
 * State shape for Gantt data received from the WebSocket stream.
 * Field names must match the JSON patch paths sent by the backend.
 */
interface GanttStreamState {
  gantt_tasks: Record<string, GanttTask>;
}

export interface UseGanttTasksResult {
  ganttTasks: FrappeGanttTask[];
  isLoading: boolean;
  error: string | null;
}

/**
 * Hook that fetches Gantt data and subscribes to real-time updates via WebSocket.
 */
export const useGanttTasks = (projectId: string | undefined): UseGanttTasksResult => {
  const endpoint = projectId
    ? `/api/projects/${encodeURIComponent(projectId)}/gantt/stream/ws`
    : undefined;

  const initialData = useCallback(
    (): GanttStreamState => ({ gantt_tasks: {} }),
    []
  );

  const { data, error } = useJsonPatchWsStream<GanttStreamState>(
    endpoint,
    Boolean(projectId),
    initialData
  );

  const tasksById = data?.gantt_tasks ?? {};

  const ganttTasks = useMemo(
    () => transformToFrappeFormat(tasksById),
    [tasksById]
  );

  const isLoading = Boolean(projectId) && !data && !error;

  return { ganttTasks, isLoading, error };
};
