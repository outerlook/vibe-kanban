import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigateWithSearch } from '@/hooks';
import { tasksApi } from '@/lib/api';
import { paths } from '@/lib/paths';
import { invalidateTaskQueries } from '@/lib/queryInvalidation';
import type {
  CreateTask,
  CreateAndStartTaskRequest,
  Task,
  TaskWithAttemptStatus,
  UpdateTask,
  SharedTaskDetails,
} from 'shared/types';

export function useTaskMutations(projectId?: string) {
  const queryClient = useQueryClient();
  const navigate = useNavigateWithSearch();

  const createTask = useMutation({
    mutationFn: (data: CreateTask) => tasksApi.create(data),
    onSuccess: (createdTask: Task) => {
      invalidateTaskQueries(queryClient, createdTask.id, {
        projectId,
        includeRelationships: !!createdTask.parent_workspace_id,
        attemptId: createdTask.parent_workspace_id ?? undefined,
      });
      if (projectId) {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err) => {
      console.error('Failed to create task:', err);
    },
  });

  const createAndStart = useMutation({
    mutationFn: (data: CreateAndStartTaskRequest) =>
      tasksApi.createAndStart(data),
    onSuccess: (createdTask: TaskWithAttemptStatus) => {
      invalidateTaskQueries(queryClient, createdTask.id, {
        projectId,
        includeRelationships: !!createdTask.parent_workspace_id,
        attemptId: createdTask.parent_workspace_id ?? undefined,
      });
      if (projectId) {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err) => {
      console.error('Failed to create and start task:', err);
    },
  });

  const updateTask = useMutation({
    mutationFn: ({ taskId, data }: { taskId: string; data: UpdateTask }) =>
      tasksApi.update(taskId, data),
    onSuccess: (updatedTask: Task) => {
      invalidateTaskQueries(queryClient, updatedTask.id, {
        projectId,
        includeDependencies: true,
      });
    },
    onError: (err) => {
      console.error('Failed to update task:', err);
    },
  });

  const deleteTask = useMutation({
    mutationFn: (taskId: string) => tasksApi.delete(taskId),
    onSuccess: (_: unknown, taskId: string) => {
      invalidateTaskQueries(queryClient, taskId, {
        projectId,
        includeDependencies: true,
        includeRelationships: true,
      });
      // Remove single-task cache entry to avoid stale data flashes
      queryClient.removeQueries({ queryKey: ['task', taskId], exact: true });
    },
    onError: (err) => {
      console.error('Failed to delete task:', err);
    },
  });

  const shareTask = useMutation({
    mutationFn: (taskId: string) => tasksApi.share(taskId),
    onError: (err) => {
      console.error('Failed to share task:', err);
    },
  });

  const unshareSharedTask = useMutation({
    mutationFn: (sharedTaskId: string) => tasksApi.unshare(sharedTaskId),
    onSuccess: () => {
      invalidateTaskQueries(queryClient, undefined, { projectId });
    },
    onError: (err) => {
      console.error('Failed to unshare task:', err);
    },
  });

  const linkSharedTaskToLocal = useMutation({
    mutationFn: (data: SharedTaskDetails) => tasksApi.linkToLocal(data),
    onSuccess: (createdTask: Task | null) => {
      console.log('Linked shared task to local successfully', createdTask);
      if (createdTask) {
        invalidateTaskQueries(queryClient, createdTask.id, { projectId });
      }
    },
    onError: (err) => {
      console.error('Failed to link shared task to local:', err);
    },
  });

  return {
    createTask,
    createAndStart,
    updateTask,
    deleteTask,
    shareTask,
    stopShareTask: unshareSharedTask,
    linkSharedTaskToLocal,
  };
}
