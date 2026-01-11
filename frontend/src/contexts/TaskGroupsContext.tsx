import { createContext, useCallback, useContext, useMemo, type ReactNode } from 'react';
import { useTaskGroups } from '@/hooks/useTaskGroups';
import { useProject } from '@/contexts/ProjectContext';
import type { TaskGroup } from 'shared/types';

interface TaskGroupsContextValue {
  groups: TaskGroup[];
  groupsById: Record<string, TaskGroup>;
  isLoading: boolean;
  getGroupName: (groupId: string | null | undefined) => string | undefined;
}

const TaskGroupsContext = createContext<TaskGroupsContextValue | null>(null);

export function TaskGroupsProvider({ children }: { children: ReactNode }) {
  const { projectId } = useProject();
  const { data: groups = [], isLoading } = useTaskGroups(projectId);

  const groupsById = useMemo(() => {
    const map: Record<string, TaskGroup> = {};
    groups.forEach((group) => {
      map[group.id] = group;
    });
    return map;
  }, [groups]);

  const getGroupName = useCallback(
    (groupId: string | null | undefined): string | undefined => {
      if (!groupId) return undefined;
      return groupsById[groupId]?.name;
    },
    [groupsById]
  );

  const value = useMemo(
    () => ({
      groups,
      groupsById,
      isLoading,
      getGroupName,
    }),
    [groups, groupsById, isLoading, getGroupName]
  );

  return (
    <TaskGroupsContext.Provider value={value}>
      {children}
    </TaskGroupsContext.Provider>
  );
}

export function useTaskGroupsContext() {
  const context = useContext(TaskGroupsContext);
  if (!context) {
    throw new Error(
      'useTaskGroupsContext must be used within a TaskGroupsProvider'
    );
  }
  return context;
}
