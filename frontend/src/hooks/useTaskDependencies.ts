import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { taskDependenciesApi } from '@/lib/api';
import type { DependencyDirection, TaskDependencyTreeNode } from '@/lib/api';
import type { Task, TaskDependency } from 'shared/types';
import { invalidateTaskQueries } from '@/lib/queryInvalidation';

export const taskDependenciesKeys = {
  all: ['taskDependencies'] as const,
  byTask: (taskId: string | undefined) =>
    ['taskDependencies', taskId] as const,
};

export const taskDependencyTreeKeys = {
  all: ['taskDependencyTree'] as const,
  byTask: (taskId: string | undefined) =>
    ['taskDependencyTree', taskId] as const,
  detail: (taskId: string | undefined, maxDepth?: number) =>
    ['taskDependencyTree', taskId, maxDepth] as const,
};

type QueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
  staleTime?: number;
  retry?: number | false;
};

export type TaskDependencySummary = {
  blocked_by: Task[];
  blocking: Task[];
};

const dependencyDirections: DependencyDirection[] = [
  'blocked_by',
  'blocking',
];

export function useTaskDependencies(taskId?: string, opts?: QueryOptions) {
  const enabled = (opts?.enabled ?? true) && !!taskId;

  return useQuery<TaskDependencySummary>({
    queryKey: taskDependenciesKeys.byTask(taskId),
    queryFn: async () => {
      const [blockedBy, blocking] = await Promise.all(
        dependencyDirections.map((direction) =>
          taskDependenciesApi.getDependencies(taskId!, direction)
        )
      );
      return {
        blocked_by: blockedBy,
        blocking,
      };
    },
    enabled,
    refetchInterval: opts?.refetchInterval ?? false,
    staleTime: opts?.staleTime ?? 10_000,
    retry: opts?.retry ?? 2,
  });
}

export function useTaskDependencyTree(
  taskId?: string,
  maxDepth?: number,
  opts?: QueryOptions
) {
  const enabled = (opts?.enabled ?? true) && !!taskId;

  return useQuery<TaskDependencyTreeNode>({
    queryKey: taskDependencyTreeKeys.detail(taskId, maxDepth),
    queryFn: () => taskDependenciesApi.getDependencyTree(taskId!, maxDepth),
    enabled,
    refetchInterval: opts?.refetchInterval ?? false,
    staleTime: opts?.staleTime ?? 10_000,
    retry: opts?.retry ?? 2,
  });
}

type DependencyMutationInput = {
  taskId: string;
  dependsOnId: string;
};

export function useAddDependency() {
  const queryClient = useQueryClient();

  return useMutation<TaskDependency, unknown, DependencyMutationInput>({
    mutationFn: ({ taskId, dependsOnId }) =>
      taskDependenciesApi.addDependency(taskId, dependsOnId),
    onSuccess: (_dependency, { taskId, dependsOnId }) => {
      // Invalidate task and dependency queries for both affected tasks
      [taskId, dependsOnId].forEach((id) => {
        invalidateTaskQueries(queryClient, id, { includeDependencies: true });
      });
    },
    onError: (err) => {
      console.error('Failed to add dependency:', err);
    },
  });
}

export function useRemoveDependency() {
  const queryClient = useQueryClient();

  return useMutation<void, unknown, DependencyMutationInput>({
    mutationFn: ({ taskId, dependsOnId }) =>
      taskDependenciesApi.removeDependency(taskId, dependsOnId),
    onSuccess: (_data, { taskId, dependsOnId }) => {
      // Invalidate task and dependency queries for both affected tasks
      [taskId, dependsOnId].forEach((id) => {
        invalidateTaskQueries(queryClient, id, { includeDependencies: true });
      });
    },
    onError: (err) => {
      console.error('Failed to remove dependency:', err);
    },
  });
}
