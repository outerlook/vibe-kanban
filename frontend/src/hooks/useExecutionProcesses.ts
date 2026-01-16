import { useCallback } from 'react';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import type { ExecutionProcess } from 'shared/types';
import { shouldShowInLogs } from '@/constants/processes';

export type ExecutionProcessesSource =
  | { type: 'workspace'; workspaceId: string }
  | { type: 'conversation'; conversationSessionId: string };

type ExecutionProcessState = {
  execution_processes: Record<string, ExecutionProcess>;
};

interface UseExecutionProcessesResult {
  executionProcesses: ExecutionProcess[];
  executionProcessesById: Record<string, ExecutionProcess>;
  isAttemptRunning: boolean;
  isLoading: boolean;
  isConnected: boolean;
  error: string | null;
}

/**
 * Stream execution processes for a workspace or conversation via WebSocket (JSON Patch) and expose as array + map.
 * Server sends initial snapshot: replace /execution_processes with an object keyed by id.
 * Live updates arrive at /execution_processes/<id> via add/replace/remove operations.
 */
export const useExecutionProcesses = (
  source: ExecutionProcessesSource | undefined,
  opts?: { showSoftDeleted?: boolean }
): UseExecutionProcessesResult => {
  const showSoftDeleted = opts?.showSoftDeleted;
  let endpoint: string | undefined;

  if (source) {
    const params = new URLSearchParams();
    if (source.type === 'workspace') {
      params.set('workspace_id', source.workspaceId);
    } else {
      params.set('conversation_session_id', source.conversationSessionId);
    }
    if (typeof showSoftDeleted === 'boolean') {
      params.set('show_soft_deleted', String(showSoftDeleted));
    }
    endpoint = `/api/execution-processes/stream/ws?${params.toString()}`;
  }

  const initialData = useCallback(
    (): ExecutionProcessState => ({ execution_processes: {} }),
    []
  );

  const { data, isConnected, error } =
    useJsonPatchWsStream<ExecutionProcessState>(endpoint, !!source, initialData);

  const executionProcessesById = data?.execution_processes ?? {};
  const executionProcesses = Object.values(executionProcessesById).sort(
    (a, b) =>
      new Date(a.created_at as unknown as string).getTime() -
      new Date(b.created_at as unknown as string).getTime()
  );
  const isAttemptRunning = executionProcesses.some(
    (process) =>
      shouldShowInLogs(process.run_reason) && process.status === 'running'
  );
  const isLoading = !!source && !data && !error; // until first snapshot

  return {
    executionProcesses,
    executionProcessesById,
    isAttemptRunning,
    isLoading,
    isConnected,
    error,
  };
};
