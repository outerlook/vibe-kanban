import { useQuery } from '@tanstack/react-query';
import { projectsApi, taskGroupsApi, tasksApi } from '@/lib/api';
import type { WorkflowAssociation, WorkflowAssociationResolution } from 'shared/types';

export const workflowAssociationKeys = {
  project: (projectId: string | undefined) => ['workflowAssociation', 'project', projectId] as const,
  taskGroup: (groupId: string | undefined) => ['workflowAssociation', 'taskGroup', groupId] as const,
  task: (taskId: string | undefined) => ['workflowAssociation', 'task', taskId] as const,
};

type QueryOptions = {
  enabled?: boolean;
  staleTime?: number;
};

export function useProjectWorkflowAssociation(
  projectId?: string,
  options?: QueryOptions
) {
  return useQuery<WorkflowAssociation | null>({
    queryKey: workflowAssociationKeys.project(projectId),
    queryFn: () => projectsApi.getWorkflowAssociation(projectId!),
    enabled: (options?.enabled ?? true) && !!projectId,
    staleTime: options?.staleTime ?? 10_000,
  });
}

export function useTaskGroupWorkflowAssociations(
  groupId?: string,
  options?: QueryOptions
) {
  return useQuery<WorkflowAssociationResolution>({
    queryKey: workflowAssociationKeys.taskGroup(groupId),
    queryFn: () => taskGroupsApi.getWorkflowAssociations(groupId!),
    enabled: (options?.enabled ?? true) && !!groupId,
    staleTime: options?.staleTime ?? 10_000,
  });
}

export function useTaskWorkflowAssociations(
  taskId?: string,
  options?: QueryOptions
) {
  return useQuery<WorkflowAssociationResolution>({
    queryKey: workflowAssociationKeys.task(taskId),
    queryFn: () => tasksApi.getWorkflowAssociations(taskId!),
    enabled: (options?.enabled ?? true) && !!taskId,
    staleTime: options?.staleTime ?? 10_000,
  });
}
