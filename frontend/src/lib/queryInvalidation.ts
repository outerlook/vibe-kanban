import type { QueryClient } from '@tanstack/react-query';
import {
  taskKeys,
  projectTasksKeys,
  taskDependenciesKeys,
  taskDependencyTreeKeys,
  taskAttemptKeys,
  taskRelationshipsKeys,
} from '@/lib/taskCacheHelpers';

export type InvalidateTaskQueriesOptions = {
  /** Project ID to invalidate project-scoped task lists */
  projectId?: string;
  /** Invalidate dependency queries (blocked_by/blocking) */
  includeDependencies?: boolean;
  /** Invalidate task attempts queries */
  includeAttempts?: boolean;
  /** Invalidate task relationships queries (subtasks/parent) */
  includeRelationships?: boolean;
  /** Attempt ID for relationship invalidation (required if includeRelationships is true) */
  attemptId?: string;
};

/**
 * Centralized task query invalidation.
 * Invalidates task-related caches based on the provided options.
 */
export function invalidateTaskQueries(
  queryClient: QueryClient,
  taskId: string | undefined,
  options: InvalidateTaskQueriesOptions = {}
): void {
  const {
    projectId,
    includeDependencies = false,
    includeAttempts = false,
    includeRelationships = false,
    attemptId,
  } = options;

  // Always invalidate the core task queries
  queryClient.invalidateQueries({ queryKey: taskKeys.all });
  if (taskId) {
    queryClient.invalidateQueries({ queryKey: taskKeys.byId(taskId) });
  }

  // Invalidate project task list if projectId provided
  if (projectId) {
    queryClient.invalidateQueries({
      queryKey: projectTasksKeys.byProject(projectId),
    });
    queryClient.invalidateQueries({
      queryKey: projectTasksKeys.byProjectInfinite(projectId),
    });
  }

  // Invalidate dependency queries
  if (includeDependencies && taskId) {
    invalidateDependencyQueries(queryClient, taskId);
  }

  // Invalidate task attempts
  if (includeAttempts && taskId) {
    queryClient.invalidateQueries({
      queryKey: taskAttemptKeys.byTask(taskId),
    });
  }

  // Invalidate task relationships
  if (includeRelationships) {
    if (attemptId) {
      queryClient.invalidateQueries({
        queryKey: taskRelationshipsKeys.byAttempt(attemptId),
      });
    } else {
      // Fallback: invalidate all relationships if no specific attemptId
      queryClient.invalidateQueries({
        queryKey: taskRelationshipsKeys.all,
      });
    }
  }
}

/**
 * Invalidate dependency-specific queries for a task.
 * Useful when adding/removing dependencies between tasks.
 */
export function invalidateDependencyQueries(
  queryClient: QueryClient,
  taskId: string
): void {
  queryClient.invalidateQueries({
    queryKey: taskDependenciesKeys.byTask(taskId),
  });
  queryClient.invalidateQueries({
    queryKey: taskDependencyTreeKeys.byTask(taskId),
  });
}

/**
 * Invalidate dependency queries for multiple tasks.
 * Useful when a dependency mutation affects both tasks.
 */
export function invalidateDependencyQueriesForTasks(
  queryClient: QueryClient,
  taskIds: string[]
): void {
  taskIds.forEach((taskId) => {
    invalidateDependencyQueries(queryClient, taskId);
  });
}

// Re-export query keys for convenience
export {
  taskKeys,
  projectTasksKeys,
  taskDependenciesKeys,
  taskDependencyTreeKeys,
  taskAttemptKeys,
  taskRelationshipsKeys,
};
