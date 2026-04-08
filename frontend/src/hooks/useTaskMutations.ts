import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigateWithSearch } from '@/hooks';
import { tasksApi } from '@/lib/api';
import { paths } from '@/lib/paths';
import { invalidateTaskQueries } from '@/lib/queryInvalidation';
import {
  applyTaskUpdatesToCache,
  taskKeys,
  projectTasksKeys,
  setTaskInCache,
  removeTaskFromCache,
  removeTasksFromCache,
  moveTaskBetweenStatuses,
  type StatusQueryData,
} from '@/lib/taskCacheHelpers';
import type {
  DeletedTaskSummary,
  CreateTask,
  CreateAndStartTaskRequest,
  Task,
  TaskStatus,
  TaskWithAttemptStatus,
  UpdateTask,
  SharedTaskDetails,
} from 'shared/types';

// Context types for optimistic update rollbacks
type UpdateTaskContext = {
  previousTask: Task | undefined;
  previousStatusData: StatusQueryData | undefined;
  oldStatus: TaskStatus | undefined;
};

type DeleteTaskContext = {
  previousTask: Task | undefined;
  previousStatusData: StatusQueryData | undefined;
  taskStatus: TaskStatus | undefined;
};

type BulkUpdateTaskStatusContext = {
  previousTasks: Task[];
  previousStatuses: Map<string, TaskStatus>;
};

type BulkDeleteTaskContext = {
  previousTasks: Task[];
};

type CreateTaskContext = {
  tempId: string;
  tempStatusData: StatusQueryData | undefined;
};

export function useTaskMutations(projectId?: string) {
  const queryClient = useQueryClient();
  const navigate = useNavigateWithSearch();

  const createTask = useMutation({
    mutationFn: (data: CreateTask) => tasksApi.create(data),
    onMutate: async (data): Promise<CreateTaskContext | undefined> => {
      if (!projectId) return undefined;

      // Generate temporary ID for optimistic task
      const tempId = `temp-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      const now = new Date().toISOString();

      // Create optimistic task
      const optimisticTask: TaskWithAttemptStatus = {
        id: tempId,
        project_id: projectId,
        title: data.title,
        description: data.description ?? null,
        status: 'todo',
        parent_workspace_id: data.parent_workspace_id ?? null,
        shared_task_id: null,
        task_group_id: data.task_group_id ?? null,
        created_at: now,
        updated_at: now,
        is_blocked: false,
        has_in_progress_attempt: false,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: '',
        needs_attention: null,
      };

      // Snapshot for potential cleanup
      const tempStatusData = queryClient.getQueryData<StatusQueryData>(
        projectTasksKeys.byProjectAndStatus(projectId, 'todo')
      );

      // Add optimistic task to cache
      setTaskInCache(queryClient, optimisticTask, projectId);

      return { tempId, tempStatusData };
    },
    onSuccess: (createdTask: Task, _data, context) => {
      // Remove temp task and add real task
      if (context?.tempId && projectId) {
        removeTaskFromCache(queryClient, context.tempId, projectId, 'todo');
        setTaskInCache(
          queryClient,
          createdTask as TaskWithAttemptStatus,
          projectId
        );
      }

      // Invalidate relationships if task has a parent
      if (createdTask.parent_workspace_id) {
        invalidateTaskQueries(queryClient, createdTask.id, {
          includeRelationships: true,
          attemptId: createdTask.parent_workspace_id,
        });
      }

      if (projectId) {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err, _data, context) => {
      console.error('Failed to create task:', err);

      // Remove optimistic task from cache
      if (context?.tempId && projectId) {
        removeTaskFromCache(queryClient, context.tempId, projectId, 'todo');
      }
    },
  });

  const createAndStart = useMutation({
    mutationFn: (data: CreateAndStartTaskRequest) =>
      tasksApi.createAndStart(data),
    onMutate: async (data): Promise<CreateTaskContext | undefined> => {
      if (!projectId) return undefined;

      // Generate temporary ID for optimistic task
      const tempId = `temp-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      const now = new Date().toISOString();
      const taskData = data.task;

      // Create optimistic task (starts as inprogress since it's create+start)
      const optimisticTask: TaskWithAttemptStatus = {
        id: tempId,
        project_id: projectId,
        title: taskData.title,
        description: taskData.description ?? null,
        status: 'inprogress',
        parent_workspace_id: taskData.parent_workspace_id ?? null,
        shared_task_id: null,
        task_group_id: taskData.task_group_id ?? null,
        created_at: now,
        updated_at: now,
        is_blocked: false,
        has_in_progress_attempt: true,
        last_attempt_failed: false,
        is_queued: false,
        last_executor: data.executor_profile_id?.executor ?? '',
        needs_attention: null,
      };

      // Snapshot for potential cleanup
      const tempStatusData = queryClient.getQueryData<StatusQueryData>(
        projectTasksKeys.byProjectAndStatus(projectId, 'inprogress')
      );

      // Add optimistic task to cache
      setTaskInCache(queryClient, optimisticTask, projectId);

      return { tempId, tempStatusData };
    },
    onSuccess: (createdTask: TaskWithAttemptStatus, _data, context) => {
      // Remove temp task and add real task
      if (context?.tempId && projectId) {
        removeTaskFromCache(
          queryClient,
          context.tempId,
          projectId,
          'inprogress'
        );
        setTaskInCache(queryClient, createdTask, projectId);
      }

      // Invalidate relationships if task has a parent
      if (createdTask.parent_workspace_id) {
        invalidateTaskQueries(queryClient, createdTask.id, {
          includeRelationships: true,
          attemptId: createdTask.parent_workspace_id,
        });
      }

      if (projectId) {
        navigate(`${paths.task(projectId, createdTask.id)}/attempts/latest`);
      }
    },
    onError: (err, _data, context) => {
      console.error('Failed to create and start task:', err);

      // Remove optimistic task from cache
      if (context?.tempId && projectId) {
        removeTaskFromCache(
          queryClient,
          context.tempId,
          projectId,
          'inprogress'
        );
      }
    },
  });

  const updateTask = useMutation({
    mutationFn: ({ taskId, data }: { taskId: string; data: UpdateTask }) =>
      tasksApi.update(taskId, data),
    onMutate: async ({
      taskId,
      data,
    }): Promise<UpdateTaskContext | undefined> => {
      if (!projectId) return undefined;

      // Cancel any outgoing refetches to avoid overwriting optimistic update
      await queryClient.cancelQueries({ queryKey: taskKeys.byId(taskId) });

      // Snapshot previous task data for rollback
      const previousTask = queryClient.getQueryData<Task>(
        taskKeys.byId(taskId)
      );
      if (!previousTask) return undefined;

      const oldStatus = previousTask.status;
      const newStatus = data.status ?? oldStatus;

      // Snapshot the old status list for rollback
      const previousStatusData = queryClient.getQueryData<StatusQueryData>(
        projectTasksKeys.byProjectAndStatus(projectId, oldStatus)
      );

      // Create optimistic task with updated fields
      const optimisticTask: TaskWithAttemptStatus = {
        ...previousTask,
        ...data,
        updated_at: new Date().toISOString(),
      } as TaskWithAttemptStatus;

      // Apply optimistic update
      if (oldStatus !== newStatus) {
        moveTaskBetweenStatuses(
          queryClient,
          optimisticTask,
          oldStatus,
          newStatus,
          projectId
        );
      } else {
        setTaskInCache(queryClient, optimisticTask, projectId);
      }

      return { previousTask, previousStatusData, oldStatus };
    },
    onError: (err, { data }, context) => {
      console.error('Failed to update task:', err);

      // Rollback to previous state
      if (context?.previousTask && projectId) {
        const newStatus = data.status ?? context.oldStatus;
        const oldStatus = context.oldStatus;

        if (oldStatus && newStatus && oldStatus !== newStatus) {
          // Reverse the status move
          moveTaskBetweenStatuses(
            queryClient,
            context.previousTask as TaskWithAttemptStatus,
            newStatus,
            oldStatus,
            projectId
          );
        } else {
          // Restore the previous task state
          setTaskInCache(
            queryClient,
            context.previousTask as TaskWithAttemptStatus,
            projectId
          );
        }
      }
    },
    onSettled: (updatedTask) => {
      // Invalidate dependencies since they may have changed
      if (updatedTask) {
        invalidateTaskQueries(queryClient, updatedTask.id, {
          includeDependencies: true,
        });
      }
    },
  });

  const deleteTask = useMutation({
    mutationFn: (taskId: string) => tasksApi.delete(taskId),
    onMutate: async (taskId): Promise<DeleteTaskContext | undefined> => {
      if (!projectId) return undefined;

      // Cancel any outgoing refetches
      await queryClient.cancelQueries({ queryKey: taskKeys.byId(taskId) });

      // Snapshot previous task data for rollback
      const previousTask = queryClient.getQueryData<Task>(
        taskKeys.byId(taskId)
      );
      if (!previousTask) return undefined;

      const taskStatus = previousTask.status;

      // Snapshot the status list for rollback
      const previousStatusData = queryClient.getQueryData<StatusQueryData>(
        projectTasksKeys.byProjectAndStatus(projectId, taskStatus)
      );

      // Optimistically remove from cache
      removeTaskFromCache(queryClient, taskId, projectId, taskStatus);

      return { previousTask, previousStatusData, taskStatus };
    },
    onError: (err, _taskId, context) => {
      console.error('Failed to delete task:', err);

      // Rollback: restore task to cache
      if (context?.previousTask && projectId && context.taskStatus) {
        setTaskInCache(
          queryClient,
          context.previousTask as TaskWithAttemptStatus,
          projectId
        );
      }
    },
    onSettled: (_result, _error, taskId) => {
      // Invalidate related queries
      invalidateTaskQueries(queryClient, taskId, {
        includeDependencies: true,
        includeRelationships: true,
      });
    },
  });

  const bulkUpdateTaskStatus = useMutation({
    mutationFn: ({
      taskIds,
      status,
    }: {
      taskIds: string[];
      status: TaskStatus;
    }) => {
      if (!projectId) {
        throw new Error('projectId is required for bulk task status updates');
      }

      return tasksApi.bulkUpdateStatus(projectId, {
        taskIds,
        status,
      });
    },
    onMutate: async ({
      taskIds,
      status,
    }): Promise<BulkUpdateTaskStatusContext | undefined> => {
      if (!projectId) return undefined;

      await Promise.all(
        taskIds.map((taskId) =>
          queryClient.cancelQueries({ queryKey: taskKeys.byId(taskId) })
        )
      );

      const previousTasks = taskIds
        .map((taskId) => queryClient.getQueryData<Task>(taskKeys.byId(taskId)))
        .filter((task): task is Task => Boolean(task));

      const previousStatuses = new Map(
        previousTasks.map((task) => [task.id, task.status])
      );

      previousTasks.forEach((task) => {
        const optimisticTask = {
          ...task,
          status,
          updated_at: new Date().toISOString(),
        } as TaskWithAttemptStatus;

        if (task.status !== status) {
          moveTaskBetweenStatuses(
            queryClient,
            optimisticTask,
            task.status,
            status,
            projectId
          );
          return;
        }

        setTaskInCache(queryClient, optimisticTask, projectId);
      });

      return { previousTasks, previousStatuses };
    },
    onSuccess: (result, _variables, context) => {
      if (!projectId) return;

      applyTaskUpdatesToCache(
        queryClient,
        result.tasks,
        context?.previousStatuses ?? new Map(),
        projectId
      );
    },
    onError: (err, _variables, context) => {
      console.error('Failed to bulk update task status:', err);

      if (!projectId) return;

      context?.previousTasks.forEach((task) => {
        const currentTask = queryClient.getQueryData<Task>(
          taskKeys.byId(task.id)
        );

        if (currentTask && currentTask.status !== task.status) {
          moveTaskBetweenStatuses(
            queryClient,
            task as TaskWithAttemptStatus,
            currentTask.status,
            task.status,
            projectId
          );
          return;
        }

        setTaskInCache(queryClient, task as TaskWithAttemptStatus, projectId);
      });
    },
    onSettled: (result, _error, variables) => {
      const taskIds = result?.tasks.map((task) => task.id) ?? variables.taskIds;

      taskIds.forEach((taskId) => {
        invalidateTaskQueries(queryClient, taskId, {
          includeDependencies: true,
        });
      });
    },
  });

  const bulkDeleteTasks = useMutation({
    mutationFn: (taskIds: string[]) => {
      if (!projectId) {
        throw new Error('projectId is required for bulk task deletion');
      }

      return tasksApi.bulkDelete(projectId, {
        taskIds,
      });
    },
    onMutate: async (taskIds): Promise<BulkDeleteTaskContext | undefined> => {
      if (!projectId) return undefined;

      await Promise.all(
        taskIds.map((taskId) =>
          queryClient.cancelQueries({ queryKey: taskKeys.byId(taskId) })
        )
      );

      const previousTasks = taskIds
        .map((taskId) => queryClient.getQueryData<Task>(taskKeys.byId(taskId)))
        .filter((task): task is Task => Boolean(task));

      removeTasksFromCache(
        queryClient,
        previousTasks.map(
          (task) =>
            ({
              id: task.id,
              projectId: task.project_id,
              status: task.status,
            }) satisfies DeletedTaskSummary
        ),
        projectId
      );

      return { previousTasks };
    },
    onSuccess: (result) => {
      if (!projectId) return;
      removeTasksFromCache(queryClient, result.deletedTasks, projectId);
    },
    onError: (err, _taskIds, context) => {
      console.error('Failed to bulk delete tasks:', err);

      if (!projectId) return;

      context?.previousTasks.forEach((task) => {
        setTaskInCache(queryClient, task as TaskWithAttemptStatus, projectId);
      });
    },
    onSettled: (result, _error, taskIds) => {
      const deletedTaskIds =
        result?.deletedTasks.map((task) => task.id) ?? taskIds;

      deletedTaskIds.forEach((taskId) => {
        invalidateTaskQueries(queryClient, taskId, {
          includeDependencies: true,
          includeRelationships: true,
        });
      });
    },
  });

  const shareTask = useMutation({
    mutationFn: (taskId: string) => tasksApi.share(taskId),
    // Task cache update is handled by WebSocket stream
    onError: (err) => {
      console.error('Failed to share task:', err);
    },
  });

  const unshareSharedTask = useMutation({
    mutationFn: (sharedTaskId: string) => tasksApi.unshare(sharedTaskId),
    // Task cache update is handled by WebSocket stream
    onError: (err) => {
      console.error('Failed to unshare task:', err);
    },
  });

  const linkSharedTaskToLocal = useMutation({
    mutationFn: (data: SharedTaskDetails) => tasksApi.linkToLocal(data),
    // Task cache update is handled by WebSocket stream
    onError: (err) => {
      console.error('Failed to link shared task to local:', err);
    },
  });

  return {
    createTask,
    createAndStart,
    updateTask,
    deleteTask,
    bulkUpdateTaskStatus,
    bulkDeleteTasks,
    shareTask,
    stopShareTask: unshareSharedTask,
    linkSharedTaskToLocal,
  };
}
