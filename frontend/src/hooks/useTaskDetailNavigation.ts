import { useCallback, useEffect, useMemo } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import type { TaskWithAttemptStatus } from 'shared/types';
import type { WorkspaceWithSession } from '@/types/attempt';
import type { LayoutMode } from '@/components/layout/TasksLayout';
import { useNavigateWithSearch } from '@/hooks/useNavigateWithSearch';
import { useTaskAttemptWithSession } from '@/hooks/useTaskAttempt';
import { useTaskAttempts } from '@/hooks/useTaskAttempts';
import { useTask } from '@/hooks/useTask';

export interface UseTaskDetailNavigationOptions {
  projectId: string | undefined;
  basePath: 'tasks' | 'gantt';
}

export interface TaskDetailNavigation {
  taskId: string | undefined;
  attemptId: string | undefined;
  mode: LayoutMode;
  isTaskView: boolean;
  isPanelOpen: boolean;
  selectedTask: TaskWithAttemptStatus | null;
  attempt: WorkspaceWithSession | undefined;
  isLoading: boolean;
  openTask: (taskId: string) => void;
  openAttempt: (taskId: string, attemptId: string) => void;
  closePanel: () => void;
  setMode: (mode: LayoutMode) => void;
  cycleView: (direction: 'forward' | 'backward') => void;
}

export const useTaskDetailNavigation = (
  options: UseTaskDetailNavigationOptions
): TaskDetailNavigation => {
  const { projectId, basePath } = options;
  const { taskId, attemptId } = useParams<{
    taskId?: string;
    attemptId?: string;
  }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const navigateWithSearch = useNavigateWithSearch();

  const { data: taskData, isLoading: isTaskLoading } = useTask(taskId);
  const selectedTask = useMemo(
    () => (taskData ? (taskData as TaskWithAttemptStatus) : null),
    [taskData]
  );

  const isLatest = attemptId === 'latest';
  const { data: attempts = [], isLoading: isAttemptsLoading } = useTaskAttempts(
    taskId,
    {
      enabled: !!taskId && isLatest,
    }
  );

  const latestAttemptId = useMemo(() => {
    if (!attempts?.length) return undefined;
    return [...attempts].sort((a, b) => {
      const diff =
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
      if (diff !== 0) return diff;
      return a.id.localeCompare(b.id);
    })[0].id;
  }, [attempts]);

  const basePathPrefix = useMemo(
    () => (projectId ? `/projects/${projectId}/${basePath}` : null),
    [projectId, basePath]
  );

  const buildTaskPath = useCallback(
    (nextTaskId: string) =>
      basePathPrefix ? `${basePathPrefix}/${nextTaskId}` : null,
    [basePathPrefix]
  );

  const buildAttemptPath = useCallback(
    (nextTaskId: string, nextAttemptId: string) =>
      basePathPrefix
        ? `${basePathPrefix}/${nextTaskId}/attempts/${nextAttemptId}`
        : null,
    [basePathPrefix]
  );

  useEffect(() => {
    if (!projectId || !taskId) return;
    if (!isLatest) return;
    if (isAttemptsLoading) return;

    if (!latestAttemptId) {
      const taskPath = buildTaskPath(taskId);
      if (taskPath) {
        navigateWithSearch(taskPath, { replace: true });
      }
      return;
    }

    const attemptPath = buildAttemptPath(taskId, latestAttemptId);
    if (attemptPath) {
      navigateWithSearch(attemptPath, { replace: true });
    }
  }, [
    projectId,
    taskId,
    isLatest,
    isAttemptsLoading,
    latestAttemptId,
    navigateWithSearch,
    buildTaskPath,
    buildAttemptPath,
  ]);

  useEffect(() => {
    if (!projectId || !taskId || isTaskLoading) return;
    if (!selectedTask) {
      if (basePathPrefix) {
        navigateWithSearch(basePathPrefix, { replace: true });
      }
    }
  }, [
    projectId,
    taskId,
    isTaskLoading,
    selectedTask,
    basePathPrefix,
    navigateWithSearch,
  ]);

  const effectiveAttemptId = attemptId === 'latest' ? undefined : attemptId;
  const { data: attempt, isLoading: isAttemptLoading } =
    useTaskAttemptWithSession(effectiveAttemptId);

  const rawMode = searchParams.get('view') as LayoutMode;
  const mode: LayoutMode =
    rawMode === 'preview' || rawMode === 'diffs' ? rawMode : null;

  useEffect(() => {
    const view = searchParams.get('view');
    if (view === 'logs') {
      const params = new URLSearchParams(searchParams);
      params.set('view', 'diffs');
      setSearchParams(params, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  const setMode = useCallback(
    (newMode: LayoutMode) => {
      const params = new URLSearchParams(searchParams);
      if (newMode === null) {
        params.delete('view');
      } else {
        params.set('view', newMode);
      }
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams]
  );

  const openTask = useCallback(
    (nextTaskId: string) => {
      const taskPath = buildTaskPath(nextTaskId);
      if (!taskPath) return;
      navigateWithSearch(taskPath);
    },
    [buildTaskPath, navigateWithSearch]
  );

  const openAttempt = useCallback(
    (nextTaskId: string, nextAttemptId: string) => {
      const attemptPath = buildAttemptPath(nextTaskId, nextAttemptId);
      if (!attemptPath) return;
      navigateWithSearch(attemptPath);
    },
    [buildAttemptPath, navigateWithSearch]
  );

  const closePanel = useCallback(() => {
    if (!basePathPrefix) return;
    navigateWithSearch(basePathPrefix, { replace: true });
  }, [basePathPrefix, navigateWithSearch]);

  const cycleView = useCallback(
    (direction: 'forward' | 'backward') => {
      const order: LayoutMode[] = [null, 'preview', 'diffs'];
      const idx = order.indexOf(mode);
      const next =
        direction === 'forward'
          ? order[(idx + 1) % order.length]
          : order[(idx - 1 + order.length) % order.length];
      setMode(next);
    },
    [mode, setMode]
  );

  const isTaskView = Boolean(taskId && !effectiveAttemptId);
  const isPanelOpen = Boolean(taskId);
  const isLoading = isTaskLoading || isAttemptsLoading || isAttemptLoading;

  return {
    taskId,
    attemptId,
    mode,
    isTaskView,
    isPanelOpen,
    selectedTask,
    attempt,
    isLoading,
    openTask,
    openAttempt,
    closePanel,
    setMode,
    cycleView,
  };
};
