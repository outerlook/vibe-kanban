import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useMemo,
  ReactNode,
} from 'react';
import { useLocation } from 'react-router-dom';

interface TaskSelectionContextValue {
  toggleTask: (taskId: string) => void;
  clearSelection: () => void;
  isTaskSelected: (taskId: string) => boolean;
  selectedCount: number;
  getSelectedIds: () => string[];
}

const TaskSelectionContext = createContext<TaskSelectionContextValue | null>(
  null
);

interface TaskSelectionProviderProps {
  children: ReactNode;
}

export function TaskSelectionProvider({
  children,
}: TaskSelectionProviderProps) {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const location = useLocation();

  // Check if we're on a tasks route
  const isTasksRoute = /^\/projects\/[^/]+\/tasks/.test(location.pathname);

  // Clear selection when leaving tasks pages
  useEffect(() => {
    if (!isTasksRoute && selectedIds.size > 0) {
      setSelectedIds(new Set());
    }
  }, [isTasksRoute, selectedIds.size]);

  const toggleTask = useCallback((taskId: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(taskId)) {
        next.delete(taskId);
      } else {
        next.add(taskId);
      }
      return next;
    });
  }, []);

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  const isTaskSelected = useCallback(
    (taskId: string): boolean => selectedIds.has(taskId),
    [selectedIds]
  );

  const getSelectedIds = useCallback(
    (): string[] => Array.from(selectedIds),
    [selectedIds]
  );

  const value = useMemo<TaskSelectionContextValue>(
    () => ({
      toggleTask,
      clearSelection,
      isTaskSelected,
      selectedCount: selectedIds.size,
      getSelectedIds,
    }),
    [toggleTask, clearSelection, isTaskSelected, selectedIds.size, getSelectedIds]
  );

  return (
    <TaskSelectionContext.Provider value={value}>
      {children}
    </TaskSelectionContext.Provider>
  );
}

export function useTaskSelection(): TaskSelectionContextValue {
  const context = useContext(TaskSelectionContext);
  if (!context) {
    throw new Error(
      'useTaskSelection must be used within a TaskSelectionProvider'
    );
  }
  return context;
}
