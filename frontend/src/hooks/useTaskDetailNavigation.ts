import { useCallback, useEffect, useMemo } from 'react';
import { useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { useNavigateWithSearch } from '@/hooks/useNavigateWithSearch';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { useTaskAttempts } from '@/hooks/useTaskAttempts';
import { useTaskAttemptWithSession } from '@/hooks/useTaskAttempt';
import { paths } from '@/lib/paths';
import type { LayoutMode } from '@/components/layout/TasksLayout';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import type { TaskWithAttemptStatus } from 'shared/types';
import type { WorkspaceWithSession } from '@/types/attempt';

export type TaskDetailBasePath = 'tasks' | 'gantt';

export interface TaskDetailNavigation {
  task: TaskWithAttemptStatus | null;
  selectedTask?: TaskWithAttemptStatus | null;
  taskId?: string;
  isTaskView: boolean;
  attempt?: WorkspaceWithSession | null;
  mode?: LayoutMode;
  onModeChange?: (mode: LayoutMode) => void;
  sharedTask?: SharedTaskRecord;
  onClose: () => void;
  navigateWithSearch?: (path: string, options?: { replace?: boolean }) => void;
  basePath?: TaskDetailBasePath;
}

export interface UseTaskDetailNavigationOptions {
  projectId?: string | null;
  basePath?: TaskDetailBasePath;
}

export interface UseTaskDetailNavigationResult extends TaskDetailNavigation {
  selectedTask: TaskWithAttemptStatus | null;
  isPanelOpen: boolean;
  openTask: (taskId: string) => void;
}

const getTaskPath = (
  basePath: TaskDetailBasePath,
  projectId: string,
  taskId: string
) =>
  basePath === 'gantt'
    ? paths.ganttTask(projectId, taskId)
    : paths.task(projectId, taskId);

const getAttemptPath = (
  basePath: TaskDetailBasePath,
  projectId: string,
  taskId: string,
  attemptId: string
) =>
  basePath === 'gantt'
    ? paths.ganttAttempt(projectId, taskId, attemptId)
    : paths.attempt(projectId, taskId, attemptId);

const getBasePath = (basePath: TaskDetailBasePath, projectId: string) =>
  basePath === 'gantt'
    ? paths.projectGantt(projectId)
    : paths.projectTasks(projectId);

export function useTaskDetailNavigation({
  projectId: explicitProjectId,
  basePath = 'tasks',
}: UseTaskDetailNavigationOptions): UseTaskDetailNavigationResult {
  const navigate = useNavigate();
  const navigateWithSearch = useNavigateWithSearch();
  const [searchParams, setSearchParams] = useSearchParams();
  const { taskId, attemptId, projectId: routeProjectId } = useParams<{
    projectId: string;
    taskId?: string;
    attemptId?: string;
  }>();

  const projectId = explicitProjectId ?? routeProjectId;
  const { tasksById, isLoading: isTasksLoading } = useProjectTasks(
    projectId ?? ''
  );
  const selectedTask = taskId ? tasksById[taskId] ?? null : null;
  const isPanelOpen = Boolean(taskId);

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

  useEffect(() => {
    if (!projectId || !taskId) return;
    if (!isLatest || isAttemptsLoading) return;

    if (!latestAttemptId) {
      navigateWithSearch(getTaskPath(basePath, projectId, taskId), {
        replace: true,
      });
      return;
    }

    navigateWithSearch(
      getAttemptPath(basePath, projectId, taskId, latestAttemptId),
      {
        replace: true,
      }
    );
  }, [
    basePath,
    isAttemptsLoading,
    isLatest,
    latestAttemptId,
    navigateWithSearch,
    projectId,
    taskId,
  ]);

  useEffect(() => {
    if (!projectId || !taskId || isTasksLoading) return;
    if (selectedTask !== null) return;

    navigate(getBasePath(basePath, projectId), { replace: true });
  }, [
    basePath,
    isTasksLoading,
    navigate,
    projectId,
    selectedTask,
    taskId,
  ]);

  const effectiveAttemptId = attemptId === 'latest' ? undefined : attemptId;
  const { data: attempt } = useTaskAttemptWithSession(effectiveAttemptId);
  const isTaskView = Boolean(taskId && !effectiveAttemptId);

  const rawMode = searchParams.get('view');
  const mode: LayoutMode =
    rawMode === 'preview' || rawMode === 'diffs' ? rawMode : null;

  const onModeChange = useCallback(
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
      if (!projectId) return;
      navigateWithSearch(
        getAttemptPath(basePath, projectId, nextTaskId, 'latest')
      );
    },
    [basePath, navigateWithSearch, projectId]
  );

  const onClose = useCallback(() => {
    if (!projectId) return;
    navigate(getBasePath(basePath, projectId), { replace: true });
  }, [basePath, navigate, projectId]);

  return {
    task: selectedTask,
    selectedTask,
    taskId,
    isTaskView,
    attempt: attempt ?? null,
    mode,
    onModeChange,
    sharedTask: undefined,
    onClose,
    navigateWithSearch,
    basePath,
    isPanelOpen,
    openTask,
  };
}
