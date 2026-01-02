import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { taskDependenciesApi } from '@/lib/api';
import type { DependencyDirection, TaskDependencyTreeNode } from '@/lib/api';
import type { Task, TaskDependency } from 'shared/types';

export const taskDependenciesKeys = {
  all: ['taskDependencies'] as const,
  byTask: (taskId: string | undefined, direction?: DependencyDirection) =>
    ['taskDependencies', taskId, direction ?? 'blocked_by'] as const,
  byTaskPrefix: (taskId: string | undefined) =>
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

export function useTaskDependencies(
  taskId?: string,
  direction?: DependencyDirection,
  opts?: QueryOptions
) {
  const enabled = (opts?.enabled ?? true) && !!taskId;

  return useQuery<Task[]>({
    queryKey: taskDependenciesKeys.byTask(taskId, direction),
    queryFn: () => taskDependenciesApi.getDependencies(taskId!, direction),
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
    onSuccess: (_dependency, { taskId }) => {
      queryClient.invalidateQueries({
        queryKey: taskDependenciesKeys.byTaskPrefix(taskId),
      });
      queryClient.invalidateQueries({
        queryKey: taskDependencyTreeKeys.byTask(taskId),
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
    onSuccess: (_data, { taskId }) => {
      queryClient.invalidateQueries({
        queryKey: taskDependenciesKeys.byTaskPrefix(taskId),
      });
      queryClient.invalidateQueries({
        queryKey: taskDependencyTreeKeys.byTask(taskId),
      });
    },
    onError: (err) => {
      console.error('Failed to remove dependency:', err);
    },
  });
}
