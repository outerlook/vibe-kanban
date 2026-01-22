import { useCallback, useMemo } from 'react';
import type { WorkspaceWithSession } from 'shared/types';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';

interface WorkspacesState {
  workspaces: Record<string, WorkspaceWithSession>;
}

interface UseTaskAttemptsStreamResult {
  attempts: WorkspaceWithSession[];
  attemptsById: Record<string, WorkspaceWithSession>;
  isConnected: boolean;
  isLoading: boolean;
  error: string | null;
}

export const useTaskAttemptsStream = (
  taskId?: string
): UseTaskAttemptsStreamResult => {
  const endpoint = taskId
    ? `/api/task-attempts/stream/ws?task_id=${taskId}&include_snapshot=true`
    : undefined;

  const initialData = useCallback(
    (): WorkspacesState => ({
      workspaces: {},
    }),
    []
  );

  const { data, isConnected, error } = useJsonPatchWsStream<WorkspacesState>(
    endpoint,
    !!taskId,
    initialData
  );

  const attemptsById = useMemo(
    () => data?.workspaces ?? {},
    [data?.workspaces]
  );

  const attempts = useMemo(() => {
    return Object.values(attemptsById).sort((a, b) => {
      // Sort by created_at descending (newest first)
      return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
    });
  }, [attemptsById]);

  // isLoading = we want data (taskId exists) but don't have it yet
  const isLoading = !!taskId && data === undefined;

  return {
    attempts,
    attemptsById,
    isConnected,
    isLoading,
    error,
  };
};
