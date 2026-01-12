import { useMemo, type ReactNode } from 'react';
import { AttemptHeaderActions } from '@/components/panels/AttemptHeaderActions';
import { TaskPanelHeaderActions } from '@/components/panels/TaskPanelHeaderActions';
import { useProject } from '@/contexts/ProjectContext';
import { paths } from '@/lib/paths';
import type { TaskDetailNavigation } from '@/hooks/useTaskDetailNavigation';

export interface BreadcrumbData {
  label: string;
  isLink: boolean;
  onClick?: () => void;
}

export interface TaskPanelHeaderResult {
  breadcrumbs: BreadcrumbData[];
  headerActions: ReactNode;
  truncatedTitle: string;
}

const truncateTitle = (title: string | undefined, maxLength = 20) => {
  if (!title) return 'Task';
  if (title.length <= maxLength) return title;

  const truncated = title.substring(0, maxLength);
  const lastSpace = truncated.lastIndexOf(' ');

  return lastSpace > 0
    ? `${truncated.substring(0, lastSpace)}...`
    : `${truncated}...`;
};

export const useTaskPanelHeader = (
  navigation: TaskDetailNavigation
): TaskPanelHeaderResult => {
  const { projectId } = useProject();
  const {
    task,
    taskId,
    isTaskView,
    attempt,
    mode,
    onModeChange,
    sharedTask,
    onClose,
    navigateWithSearch,
  } = navigation;

  const truncatedTitle = useMemo(
    () => truncateTitle(task?.title),
    [task?.title]
  );

  const breadcrumbs = useMemo(() => {
    if (!task) return [];

    if (isTaskView) {
      return [{ label: truncatedTitle, isLink: false }];
    }

    const resolvedTaskId = taskId ?? task.id;
    const canNavigateToTask =
      projectId && resolvedTaskId && navigateWithSearch;

    return [
      {
        label: truncatedTitle,
        isLink: true,
        onClick: canNavigateToTask
          ? () => navigateWithSearch(paths.task(projectId, resolvedTaskId))
          : undefined,
      },
      {
        label: attempt?.branch || 'Task Attempt',
        isLink: false,
      },
    ];
  }, [
    attempt?.branch,
    isTaskView,
    navigateWithSearch,
    projectId,
    task,
    taskId,
    truncatedTitle,
  ]);

  const headerActions = useMemo<ReactNode>(() => {
    if (!task) return null;

    if (isTaskView) {
      return (
        <TaskPanelHeaderActions
          task={task}
          sharedTask={sharedTask}
          onClose={onClose}
        />
      );
    }

    return (
      <AttemptHeaderActions
        mode={mode}
        onModeChange={onModeChange}
        task={task}
        sharedTask={sharedTask}
        attempt={attempt ?? null}
        onClose={onClose}
      />
    );
  }, [
    attempt,
    isTaskView,
    mode,
    onClose,
    onModeChange,
    sharedTask,
    task,
  ]);

  return { breadcrumbs, headerActions, truncatedTitle };
};
