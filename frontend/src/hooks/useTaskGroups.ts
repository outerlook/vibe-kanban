import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { taskGroupsApi } from '@/lib/api';
import type {
  TaskGroup,
  CreateTaskGroup,
  UpdateTaskGroup,
} from 'shared/types';
import { taskKeys } from './useTask';
import { projectTasksKeys } from './useProjectTasks';

export const taskGroupKeys = {
  all: ['taskGroups'] as const,
  byProject: (projectId: string | undefined) =>
    ['taskGroups', 'project', projectId] as const,
  byId: (groupId: string | undefined) =>
    ['taskGroups', 'detail', groupId] as const,
};

export interface UseTaskGroupMutationsOptions {
  onCreateSuccess?: (group: TaskGroup) => void;
  onCreateError?: (err: unknown) => void;
  onUpdateSuccess?: (group: TaskGroup) => void;
  onUpdateError?: (err: unknown) => void;
}

type QueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
  staleTime?: number;
  retry?: number | false;
};

export function useTaskGroups(projectId?: string, opts?: QueryOptions) {
  const enabled = (opts?.enabled ?? true) && !!projectId;

  return useQuery<TaskGroup[]>({
    queryKey: taskGroupKeys.byProject(projectId),
    queryFn: () => taskGroupsApi.getByProject(projectId!),
    enabled,
    refetchInterval: opts?.refetchInterval ?? false,
    staleTime: opts?.staleTime ?? 10_000,
    retry: opts?.retry ?? 2,
  });
}

export function useTaskGroup(groupId?: string, opts?: QueryOptions) {
  const enabled = (opts?.enabled ?? true) && !!groupId;

  return useQuery<TaskGroup>({
    queryKey: taskGroupKeys.byId(groupId),
    queryFn: () => taskGroupsApi.getById(groupId!),
    enabled,
    refetchInterval: opts?.refetchInterval ?? false,
    staleTime: opts?.staleTime ?? 10_000,
    retry: opts?.retry ?? 2,
  });
}

export function useTaskGroupMutations(
  projectId: string,
  options?: UseTaskGroupMutationsOptions
) {
  const queryClient = useQueryClient();

  const createTaskGroup = useMutation<
    TaskGroup,
    unknown,
    Omit<CreateTaskGroup, 'project_id'>
  >({
    mutationKey: ['createTaskGroup', projectId],
    mutationFn: (data) =>
      taskGroupsApi.create({ ...data, project_id: projectId }),
    onSuccess: (group) => {
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byProject(projectId),
      });
      options?.onCreateSuccess?.(group);
    },
    onError: (err) => {
      console.error('Failed to create task group:', err);
      options?.onCreateError?.(err);
    },
  });

  const updateTaskGroup = useMutation<
    TaskGroup,
    unknown,
    { groupId: string; data: UpdateTaskGroup }
  >({
    mutationKey: ['updateTaskGroup'],
    mutationFn: ({ groupId, data }) => taskGroupsApi.update(groupId, data),
    onSuccess: (updatedGroup) => {
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byId(updatedGroup.id),
      });
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byProject(updatedGroup.project_id),
      });
      options?.onUpdateSuccess?.(updatedGroup);
    },
    onError: (err) => {
      console.error('Failed to update task group:', err);
      options?.onUpdateError?.(err);
    },
  });

  return {
    createTaskGroup,
    updateTaskGroup,
  };
}

export function useCreateTaskGroup(projectId: string) {
  const queryClient = useQueryClient();

  return useMutation<TaskGroup, unknown, Omit<CreateTaskGroup, 'project_id'>>({
    mutationFn: (data) =>
      taskGroupsApi.create({ ...data, project_id: projectId }),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byProject(projectId),
      });
    },
    onError: (err) => {
      console.error('Failed to create task group:', err);
    },
  });
}

export function useUpdateTaskGroup() {
  const queryClient = useQueryClient();

  return useMutation<
    TaskGroup,
    unknown,
    { groupId: string; data: UpdateTaskGroup }
  >({
    mutationFn: ({ groupId, data }) => taskGroupsApi.update(groupId, data),
    onSuccess: (updatedGroup) => {
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byId(updatedGroup.id),
      });
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byProject(updatedGroup.project_id),
      });
    },
    onError: (err) => {
      console.error('Failed to update task group:', err);
    },
  });
}

export function useDeleteTaskGroup() {
  const queryClient = useQueryClient();

  return useMutation<void, unknown, { groupId: string; projectId: string }>({
    mutationFn: ({ groupId }) => taskGroupsApi.delete(groupId),
    onSuccess: (_data, { groupId, projectId }) => {
      queryClient.removeQueries({
        queryKey: taskGroupKeys.byId(groupId),
      });
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byProject(projectId),
      });
      // Invalidate tasks since they may reference this group
      queryClient.invalidateQueries({
        queryKey: taskKeys.all,
      });
      // Invalidate project tasks for kanban board refresh
      queryClient.invalidateQueries({
        queryKey: projectTasksKeys.byProject(projectId),
      });
    },
    onError: (err) => {
      console.error('Failed to delete task group:', err);
    },
  });
}

export function useAssignTasksToGroup() {
  const queryClient = useQueryClient();

  return useMutation<
    void,
    unknown,
    { groupId: string; taskIds: string[]; projectId: string }
  >({
    mutationFn: ({ groupId, taskIds }) =>
      taskGroupsApi.assignTasks(groupId, taskIds),
    onSuccess: (_data, { taskIds, projectId }) => {
      // Invalidate individual tasks that were assigned
      taskIds.forEach((taskId) => {
        queryClient.invalidateQueries({
          queryKey: taskKeys.byId(taskId),
        });
      });
      // Invalidate task list since task_group_id changed
      queryClient.invalidateQueries({
        queryKey: taskKeys.all,
      });
      // Invalidate task groups for the project
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byProject(projectId),
      });
      // Invalidate project tasks for kanban board refresh
      queryClient.invalidateQueries({
        queryKey: projectTasksKeys.byProject(projectId),
      });
    },
    onError: (err) => {
      console.error('Failed to assign tasks to group:', err);
    },
  });
}

export function useMergeTaskGroup() {
  const queryClient = useQueryClient();

  return useMutation<
    TaskGroup,
    unknown,
    { sourceId: string; targetId: string; projectId: string }
  >({
    mutationFn: ({ sourceId, targetId }) =>
      taskGroupsApi.merge(sourceId, targetId),
    onSuccess: (_data, { sourceId, projectId }) => {
      // Remove the source group from cache
      queryClient.removeQueries({
        queryKey: taskGroupKeys.byId(sourceId),
      });
      // Invalidate task groups list
      queryClient.invalidateQueries({
        queryKey: taskGroupKeys.byProject(projectId),
      });
      // Invalidate tasks since their group assignments changed
      queryClient.invalidateQueries({
        queryKey: taskKeys.all,
      });
      // Invalidate project tasks for kanban board refresh
      queryClient.invalidateQueries({
        queryKey: projectTasksKeys.byProject(projectId),
      });
    },
    onError: (err) => {
      console.error('Failed to merge task groups:', err);
    },
  });
}
