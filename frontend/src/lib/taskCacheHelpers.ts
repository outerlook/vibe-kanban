import type { QueryClient } from '@tanstack/react-query';
import type { TaskStatus, TaskWithAttemptStatus, Task } from 'shared/types';

// ============================================================================
// Query Keys - Consolidated from multiple locations
// ============================================================================

/**
 * Query keys for individual task queries (by ID).
 * Previously in: frontend/src/hooks/useTask.ts
 */
export const taskKeys = {
  all: ['tasks'] as const,
  byId: (taskId: string | undefined) => ['tasks', taskId] as const,
};

/**
 * Query keys for project-scoped task list queries.
 * Previously in: frontend/src/contexts/ProjectTasksContext.tsx
 */
export const projectTasksKeys = {
  all: ['projectTasks'] as const,
  byProject: (projectId: string | undefined) =>
    ['projectTasks', projectId] as const,
  byProjectInfinite: (projectId: string | undefined) =>
    ['projectTasks', 'infinite', projectId] as const,
  byProjectAndStatus: (projectId: string | undefined, status: TaskStatus) =>
    ['projectTasks', projectId, 'status', status] as const,
};

/**
 * Query keys for task dependency queries.
 */
export const taskDependenciesKeys = {
  all: ['taskDependencies'] as const,
  byTask: (taskId: string | undefined) =>
    ['taskDependencies', taskId] as const,
};

/**
 * Query keys for task dependency tree queries.
 */
export const taskDependencyTreeKeys = {
  all: ['taskDependencyTree'] as const,
  byTask: (taskId: string | undefined) =>
    ['taskDependencyTree', taskId] as const,
  detail: (taskId: string | undefined, maxDepth?: number) =>
    ['taskDependencyTree', taskId, maxDepth] as const,
};

/**
 * Query keys for task attempts queries.
 */
export const taskAttemptKeys = {
  all: ['taskAttempts'] as const,
  byTask: (taskId: string | undefined) => ['taskAttempts', taskId] as const,
};

/**
 * Query keys for task relationships queries.
 */
export const taskRelationshipsKeys = {
  all: ['taskRelationships'] as const,
  byAttempt: (attemptId: string | undefined) =>
    ['taskRelationships', attemptId] as const,
};

// ============================================================================
// Cache Data Types
// ============================================================================

/**
 * The structure of data stored in per-status query cache.
 * Matches the shape returned by tasksApi.list() for a specific status.
 */
export type StatusQueryData = {
  status: TaskStatus;
  page: {
    tasks: TaskWithAttemptStatus[];
    total: number;
    hasMore: boolean;
  };
};

// ============================================================================
// Cache Helper Functions
// ============================================================================

/**
 * Check if a task update should be applied based on timestamp comparison.
 * Returns true if the new task is newer or timestamps are equal.
 * This provides idempotency - stale updates are ignored.
 */
function shouldApplyUpdate(
  existingTask: TaskWithAttemptStatus | Task | undefined,
  newTask: TaskWithAttemptStatus | Task
): boolean {
  if (!existingTask) {
    return true;
  }
  const existingTime = new Date(existingTask.updated_at).getTime();
  const newTime = new Date(newTask.updated_at).getTime();
  return newTime >= existingTime;
}

/**
 * Get the sort comparator for a given status.
 * - 'done' and 'cancelled' are sorted by updated_at descending (most recent first)
 * - All other statuses are sorted by created_at ascending (oldest first)
 */
function getSortComparator(status: TaskStatus): (a: TaskWithAttemptStatus, b: TaskWithAttemptStatus) => number {
  if (status === 'done' || status === 'cancelled') {
    return (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
  }
  return (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
}

/**
 * Add or update a task in all relevant caches.
 *
 * Updates:
 * - The per-status list query cache (projectTasksKeys.byProjectAndStatus)
 * - The single-task query cache (taskKeys.byId)
 *
 * @param queryClient - The React Query client
 * @param task - The task to add or update
 * @param projectId - The project ID the task belongs to
 */
export function setTaskInCache(
  queryClient: QueryClient,
  task: TaskWithAttemptStatus,
  projectId: string
): void {
  const status = task.status;
  const queryKey = projectTasksKeys.byProjectAndStatus(projectId, status);

  // Update per-status list cache
  queryClient.setQueryData<StatusQueryData>(queryKey, (oldData) => {
    if (!oldData) {
      // Cache doesn't exist yet - create it with the new task
      return {
        status,
        page: {
          tasks: [task],
          total: 1,
          hasMore: false,
        },
      };
    }

    const existingIndex = oldData.page.tasks.findIndex((t) => t.id === task.id);

    if (existingIndex >= 0) {
      // Task exists - check timestamp before updating
      const existingTask = oldData.page.tasks[existingIndex];
      if (!shouldApplyUpdate(existingTask, task)) {
        return oldData; // Stale update, ignore
      }

      // Update in place
      const newTasks = [...oldData.page.tasks];
      newTasks[existingIndex] = task;
      // Re-sort after update (in case updated_at changed sort position)
      newTasks.sort(getSortComparator(status));

      return {
        ...oldData,
        page: {
          ...oldData.page,
          tasks: newTasks,
        },
      };
    } else {
      // Task doesn't exist in this status list - add it
      const newTasks = [...oldData.page.tasks, task];
      newTasks.sort(getSortComparator(status));

      return {
        ...oldData,
        page: {
          ...oldData.page,
          tasks: newTasks,
          total: oldData.page.total + 1,
        },
      };
    }
  });

  // Update single-task cache
  queryClient.setQueryData<Task>(taskKeys.byId(task.id), (oldTask) => {
    if (oldTask && !shouldApplyUpdate(oldTask, task)) {
      return oldTask; // Stale update, ignore
    }
    return task;
  });
}

/**
 * Remove a task from all relevant caches.
 *
 * Updates:
 * - The per-status list query cache (projectTasksKeys.byProjectAndStatus)
 * - Removes the single-task query cache entry (taskKeys.byId)
 *
 * @param queryClient - The React Query client
 * @param taskId - The ID of the task to remove
 * @param projectId - The project ID the task belongs to
 * @param status - The status the task is currently in
 */
export function removeTaskFromCache(
  queryClient: QueryClient,
  taskId: string,
  projectId: string,
  status: TaskStatus
): void {
  const queryKey = projectTasksKeys.byProjectAndStatus(projectId, status);

  // Remove from per-status list cache
  queryClient.setQueryData<StatusQueryData>(queryKey, (oldData) => {
    if (!oldData) {
      return oldData;
    }

    const existingIndex = oldData.page.tasks.findIndex((t) => t.id === taskId);
    if (existingIndex < 0) {
      return oldData; // Task not found, nothing to do
    }

    const newTasks = oldData.page.tasks.filter((t) => t.id !== taskId);

    return {
      ...oldData,
      page: {
        ...oldData.page,
        tasks: newTasks,
        total: Math.max(0, oldData.page.total - 1),
      },
    };
  });

  // Remove single-task cache
  queryClient.removeQueries({ queryKey: taskKeys.byId(taskId) });
}

/**
 * Move a task between status lists in the cache.
 *
 * This is an optimized operation for status transitions that:
 * 1. Removes the task from the old status list
 * 2. Adds the task to the new status list
 * 3. Updates pagination totals for both
 * 4. Updates the single-task cache
 *
 * @param queryClient - The React Query client
 * @param task - The task with its NEW status already set
 * @param oldStatus - The status the task is moving FROM
 * @param newStatus - The status the task is moving TO (should match task.status)
 * @param projectId - The project ID the task belongs to
 */
export function moveTaskBetweenStatuses(
  queryClient: QueryClient,
  task: TaskWithAttemptStatus,
  oldStatus: TaskStatus,
  newStatus: TaskStatus,
  projectId: string
): void {
  if (oldStatus === newStatus) {
    // No actual status change - just update in place
    setTaskInCache(queryClient, task, projectId);
    return;
  }

  // Remove from old status list
  const oldQueryKey = projectTasksKeys.byProjectAndStatus(projectId, oldStatus);
  queryClient.setQueryData<StatusQueryData>(oldQueryKey, (oldData) => {
    if (!oldData) {
      return oldData;
    }

    const existingTask = oldData.page.tasks.find((t) => t.id === task.id);
    if (existingTask && !shouldApplyUpdate(existingTask, task)) {
      return oldData; // Stale update, ignore
    }

    const newTasks = oldData.page.tasks.filter((t) => t.id !== task.id);
    const wasRemoved = newTasks.length < oldData.page.tasks.length;

    return {
      ...oldData,
      page: {
        ...oldData.page,
        tasks: newTasks,
        total: wasRemoved ? Math.max(0, oldData.page.total - 1) : oldData.page.total,
      },
    };
  });

  // Add to new status list
  const newQueryKey = projectTasksKeys.byProjectAndStatus(projectId, newStatus);
  queryClient.setQueryData<StatusQueryData>(newQueryKey, (oldData) => {
    if (!oldData) {
      // Cache doesn't exist yet - create it with the task
      return {
        status: newStatus,
        page: {
          tasks: [task],
          total: 1,
          hasMore: false,
        },
      };
    }

    // Check if task already exists (race condition protection)
    const existingIndex = oldData.page.tasks.findIndex((t) => t.id === task.id);
    if (existingIndex >= 0) {
      const existingTask = oldData.page.tasks[existingIndex];
      if (!shouldApplyUpdate(existingTask, task)) {
        return oldData; // Stale update, ignore
      }
      // Update in place
      const newTasks = [...oldData.page.tasks];
      newTasks[existingIndex] = task;
      newTasks.sort(getSortComparator(newStatus));
      return {
        ...oldData,
        page: {
          ...oldData.page,
          tasks: newTasks,
        },
      };
    }

    // Add new task
    const newTasks = [...oldData.page.tasks, task];
    newTasks.sort(getSortComparator(newStatus));

    return {
      ...oldData,
      page: {
        ...oldData.page,
        tasks: newTasks,
        total: oldData.page.total + 1,
      },
    };
  });

  // Update single-task cache
  queryClient.setQueryData<Task>(taskKeys.byId(task.id), (oldTask) => {
    if (oldTask && !shouldApplyUpdate(oldTask, task)) {
      return oldTask; // Stale update, ignore
    }
    return task;
  });
}
