import type { QueryClient } from '@tanstack/react-query';
import {
  taskDependenciesKeys,
  taskDependencyTreeKeys,
  taskAttemptKeys,
  taskRelationshipsKeys,
} from '@/lib/taskCacheHelpers';

// Re-export for backwards compatibility with existing imports
export {
  taskDependenciesKeys,
  taskDependencyTreeKeys,
  taskAttemptKeys,
  taskRelationshipsKeys,
};

export type InvalidateTaskQueriesOptions = {
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
 * Invalidate task-related queries that are NOT handled by WebSocket.
 *
 * Note: Task CRUD operations (create, update, delete) are handled by WebSocket
 * and optimistic updates, so this function only handles:
 * - Dependencies (blocked_by/blocking)
 * - Task attempts
 * - Task relationships (subtasks/parent)
 */
export function invalidateTaskQueries(
  queryClient: QueryClient,
  taskId: string | undefined,
  options: InvalidateTaskQueriesOptions = {}
): void {
  const {
    includeDependencies = false,
    includeAttempts = false,
    includeRelationships = false,
    attemptId,
  } = options;

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

