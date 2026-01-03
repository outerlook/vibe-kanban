import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useMemo,
  ReactNode,
} from 'react';
import type { TaskWithAttemptStatus } from 'shared/types';

const STORAGE_KEY = 'vibe-kanban:unread';

interface UnreadState {
  acknowledged: string[];
  projectCounts: Record<string, number>;
}

interface UnreadContextValue {
  isTaskUnread: (task: TaskWithAttemptStatus) => boolean;
  markTaskAsRead: (taskId: string) => void;
  getProjectUnreadCount: (projectId: string) => number | undefined;
  updateProjectUnreadCount: (projectId: string, count: number) => void;
  clearTaskAcknowledgment: (taskId: string) => void;
}

const UnreadContext = createContext<UnreadContextValue | null>(null);

function loadState(): UnreadState {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (!saved) return { acknowledged: [], projectCounts: {} };
    const parsed = JSON.parse(saved);
    return {
      acknowledged: Array.isArray(parsed.acknowledged) ? parsed.acknowledged : [],
      projectCounts: typeof parsed.projectCounts === 'object' && parsed.projectCounts !== null
        ? parsed.projectCounts
        : {},
    };
  } catch {
    return { acknowledged: [], projectCounts: {} };
  }
}

function saveState(state: UnreadState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Ignore storage errors
  }
}

interface UnreadProviderProps {
  children: ReactNode;
}

export function UnreadProvider({ children }: UnreadProviderProps) {
  const [state, setState] = useState<UnreadState>(loadState);

  // Persist to localStorage whenever state changes
  useEffect(() => {
    saveState(state);
  }, [state]);

  const acknowledgedSet = useMemo(
    () => new Set(state.acknowledged),
    [state.acknowledged]
  );

  const isTaskUnread = useCallback(
    (task: TaskWithAttemptStatus): boolean => {
      if (task.status !== 'inreview') return false;
      return !acknowledgedSet.has(task.id);
    },
    [acknowledgedSet]
  );

  const markTaskAsRead = useCallback((taskId: string) => {
    setState((prev) => {
      if (prev.acknowledged.includes(taskId)) return prev;
      return {
        ...prev,
        acknowledged: [...prev.acknowledged, taskId],
      };
    });
  }, []);

  const clearTaskAcknowledgment = useCallback((taskId: string) => {
    setState((prev) => {
      if (!prev.acknowledged.includes(taskId)) return prev;
      return {
        ...prev,
        acknowledged: prev.acknowledged.filter((id) => id !== taskId),
      };
    });
  }, []);

  const getProjectUnreadCount = useCallback(
    (projectId: string): number | undefined => {
      return state.projectCounts[projectId];
    },
    [state.projectCounts]
  );

  const updateProjectUnreadCount = useCallback(
    (projectId: string, count: number) => {
      setState((prev) => {
        if (prev.projectCounts[projectId] === count) return prev;
        return {
          ...prev,
          projectCounts: {
            ...prev.projectCounts,
            [projectId]: count,
          },
        };
      });
    },
    []
  );

  const value = useMemo<UnreadContextValue>(
    () => ({
      isTaskUnread,
      markTaskAsRead,
      getProjectUnreadCount,
      updateProjectUnreadCount,
      clearTaskAcknowledgment,
    }),
    [
      isTaskUnread,
      markTaskAsRead,
      getProjectUnreadCount,
      updateProjectUnreadCount,
      clearTaskAcknowledgment,
    ]
  );

  return (
    <UnreadContext.Provider value={value}>{children}</UnreadContext.Provider>
  );
}

export function useUnread(): UnreadContextValue {
  const context = useContext(UnreadContext);
  if (!context) {
    throw new Error('useUnread must be used within an UnreadProvider');
  }
  return context;
}
