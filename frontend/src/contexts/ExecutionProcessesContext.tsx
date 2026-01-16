import React, { createContext, useContext, useMemo } from 'react';
import {
  useExecutionProcesses,
  type ExecutionProcessesSource,
} from '@/hooks/useExecutionProcesses';
import type { ExecutionProcess } from 'shared/types';
import { shouldShowInLogs } from '@/constants/processes';

type ExecutionProcessesContextType = {
  executionProcessesAll: ExecutionProcess[];
  executionProcessesByIdAll: Record<string, ExecutionProcess>;
  isAttemptRunningAll: boolean;

  executionProcessesVisible: ExecutionProcess[];
  executionProcessesByIdVisible: Record<string, ExecutionProcess>;
  isAttemptRunningVisible: boolean;

  isLoading: boolean;
  isConnected: boolean;
  error: string | null;
};

const ExecutionProcessesContext =
  createContext<ExecutionProcessesContextType | null>(null);

export const ExecutionProcessesProvider: React.FC<{
  source: ExecutionProcessesSource | undefined;
  children: React.ReactNode;
}> = ({ source, children }) => {
  const {
    executionProcesses,
    executionProcessesById,
    isAttemptRunning,
    isLoading,
    isConnected,
    error,
  } = useExecutionProcesses(source, { showSoftDeleted: true });

  const visible = useMemo(
    () => executionProcesses.filter((p) => !p.dropped),
    [executionProcesses]
  );

  const executionProcessesByIdVisible = useMemo(() => {
    const m: Record<string, ExecutionProcess> = {};
    for (const p of visible) m[p.id] = p;
    return m;
  }, [visible]);

  const isAttemptRunningVisible = useMemo(
    () =>
      visible.some(
        (process) =>
          shouldShowInLogs(process.run_reason) && process.status === 'running'
      ),
    [visible]
  );

  const value = useMemo<ExecutionProcessesContextType>(
    () => ({
      executionProcessesAll: executionProcesses,
      executionProcessesByIdAll: executionProcessesById,
      isAttemptRunningAll: isAttemptRunning,
      executionProcessesVisible: visible,
      executionProcessesByIdVisible,
      isAttemptRunningVisible,
      isLoading,
      isConnected,
      error,
    }),
    [
      executionProcesses,
      executionProcessesById,
      isAttemptRunning,
      visible,
      executionProcessesByIdVisible,
      isAttemptRunningVisible,
      isLoading,
      isConnected,
      error,
    ]
  );

  return (
    <ExecutionProcessesContext.Provider value={value}>
      {children}
    </ExecutionProcessesContext.Provider>
  );
};

export const useExecutionProcessesContext = () => {
  const ctx = useContext(ExecutionProcessesContext);
  if (!ctx) {
    throw new Error(
      'useExecutionProcessesContext must be used within ExecutionProcessesProvider'
    );
  }
  return ctx;
};
