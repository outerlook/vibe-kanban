import { useMemo } from 'react';
import type { TaskStatus, TaskWithAttemptStatus } from 'shared/types';
import type { SharedTaskRecord } from './useProjectTasks';
import type { KanbanColumnItem } from '@/components/tasks/TaskKanbanBoard';
import type { TaskFilters } from './useTaskFilters';
import { TASK_STATUSES, normalizeStatus } from '@/constants/taskStatuses';

export interface UseFilteredTasksResult {
  kanbanColumns: Record<TaskStatus, KanbanColumnItem[]>;
  visibleTasksByStatus: Record<TaskStatus, TaskWithAttemptStatus[]>;
  hasVisibleLocalTasks: boolean;
  hasVisibleSharedTasks: boolean;
}

interface UseFilteredTasksParams {
  tasks: TaskWithAttemptStatus[];
  sharedTasksById: Record<string, SharedTaskRecord>;
  sharedOnlyByStatus: Record<TaskStatus, SharedTaskRecord[]>;
  filters: TaskFilters;
  showSharedTasks: boolean;
  userId: string | null;
}

export function useFilteredTasks({
  tasks,
  sharedTasksById,
  sharedOnlyByStatus,
  filters,
  showSharedTasks,
  userId,
}: UseFilteredTasksParams): UseFilteredTasksResult {
  const hasSearch = Boolean(filters.search.trim());
  const normalizedSearch = filters.search.trim().toLowerCase();

  const kanbanColumns = useMemo(() => {
    const columns: Record<TaskStatus, KanbanColumnItem[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    const matchesSearch = (
      title: string,
      description?: string | null
    ): boolean => {
      if (!hasSearch) return true;
      const lowerTitle = title.toLowerCase();
      const lowerDescription = description?.toLowerCase() ?? '';
      return (
        lowerTitle.includes(normalizedSearch) ||
        lowerDescription.includes(normalizedSearch)
      );
    };

    const matchesGroup = (taskGroupId: string | null): boolean => {
      if (filters.groupId === null) return true;
      return taskGroupId === filters.groupId;
    };

    const matchesStatus = (status: TaskStatus): boolean => {
      if (filters.statuses.length === 0) return true;
      return filters.statuses.includes(status);
    };

    tasks.forEach((task) => {
      const statusKey = normalizeStatus(task.status);
      const sharedTask = task.shared_task_id
        ? sharedTasksById[task.shared_task_id]
        : sharedTasksById[task.id];

      if (!matchesSearch(task.title, task.description)) {
        return;
      }

      if (!matchesGroup(task.task_group_id)) {
        return;
      }

      if (!matchesStatus(statusKey)) {
        return;
      }

      const isSharedAssignedElsewhere =
        !showSharedTasks &&
        !!sharedTask &&
        !!sharedTask.assignee_user_id &&
        sharedTask.assignee_user_id !== userId;

      if (isSharedAssignedElsewhere) {
        return;
      }

      columns[statusKey].push({
        type: 'task',
        task,
        sharedTask,
      });
    });

    (
      Object.entries(sharedOnlyByStatus) as [TaskStatus, SharedTaskRecord[]][]
    ).forEach(([status, items]) => {
      if (!columns[status]) {
        columns[status] = [];
      }

      if (!matchesStatus(status)) {
        return;
      }

      items.forEach((sharedTask) => {
        if (!matchesSearch(sharedTask.title, sharedTask.description)) {
          return;
        }
        const shouldIncludeShared =
          showSharedTasks || sharedTask.assignee_user_id === userId;
        if (!shouldIncludeShared) {
          return;
        }
        columns[status].push({
          type: 'shared',
          task: sharedTask,
        });
      });
    });

    TASK_STATUSES.forEach((status) => {
      columns[status].sort((a, b) => {
        const aTime = new Date(a.task.created_at).getTime();
        const bTime = new Date(b.task.created_at).getTime();
        return bTime - aTime;
      });
    });

    return columns;
  }, [
    hasSearch,
    normalizedSearch,
    tasks,
    sharedOnlyByStatus,
    sharedTasksById,
    showSharedTasks,
    userId,
    filters.groupId,
    filters.statuses,
  ]);

  const visibleTasksByStatus = useMemo(() => {
    const map: Record<TaskStatus, TaskWithAttemptStatus[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    TASK_STATUSES.forEach((status) => {
      map[status] = kanbanColumns[status]
        .filter((item): item is KanbanColumnItem & { type: 'task' } => item.type === 'task')
        .map((item) => item.task);
    });

    return map;
  }, [kanbanColumns]);

  const hasVisibleLocalTasks = useMemo(
    () =>
      Object.values(visibleTasksByStatus).some(
        (items) => items && items.length > 0
      ),
    [visibleTasksByStatus]
  );

  const hasVisibleSharedTasks = useMemo(
    () =>
      Object.values(kanbanColumns).some((items) =>
        items.some((item) => item.type === 'shared')
      ),
    [kanbanColumns]
  );

  return {
    kanbanColumns,
    visibleTasksByStatus,
    hasVisibleLocalTasks,
    hasVisibleSharedTasks,
  };
}
