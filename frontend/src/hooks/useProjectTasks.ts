import { useCallback, useMemo } from 'react';
import { useAuth } from '@/hooks';
import { useProject } from '@/contexts/ProjectContext';
import { useLiveQuery, eq, isNull } from '@tanstack/react-db';
import { sharedTasksCollection } from '@/lib/electric/sharedTasksCollection';
import { useAssigneeUserNames } from './useAssigneeUserName';
import { useAutoLinkSharedTasks } from './useAutoLinkSharedTasks';
import { useProjectTasksContext } from '@/contexts/ProjectTasksContext';
import type {
  SharedTask,
  TaskStatus,
  TaskWithAttemptStatus,
} from 'shared/types';

const ALL_STATUSES: TaskStatus[] = ['todo', 'inprogress', 'inreview', 'done', 'cancelled'];

export type SharedTaskRecord = SharedTask & {
  remote_project_id: string;
  assignee_first_name?: string | null;
  assignee_last_name?: string | null;
  assignee_username?: string | null;
};

const sortTasksForStatus = <T extends { created_at: string | Date; updated_at: string | Date }>(
  tasks: T[],
  status: TaskStatus
): void => {
  const orderBy = status === 'done' || status === 'cancelled' ? 'updated_at_desc' : 'created_at_asc';
  if (orderBy === 'updated_at_desc') {
    tasks.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
  } else {
    tasks.sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime());
  }
};

export interface UseProjectTasksResult {
  tasks: TaskWithAttemptStatus[];
  tasksById: Record<string, TaskWithAttemptStatus>;
  tasksByStatus: Record<TaskStatus, TaskWithAttemptStatus[]>;
  sharedTasksById: Record<string, SharedTaskRecord>;
  sharedOnlyByStatus: Record<TaskStatus, SharedTaskRecord[]>;
  isLoading: boolean;
  isInitialSyncComplete: boolean;
  error: string | null;
  loadMoreByStatus: Record<TaskStatus, () => void>;
  isLoadingMoreByStatus: Record<TaskStatus, boolean>;
  hasMoreByStatus: Record<TaskStatus, boolean>;
  totalByStatus: Record<TaskStatus, number>;
}

export const useProjectTasks = (projectId: string): UseProjectTasksResult => {
  const { project } = useProject();
  const { isSignedIn } = useAuth();
  const remoteProjectId = project?.remote_project_id;

  // Get state from centralized context
  const {
    tasksById,
    paginationByStatus,
    isQueriesLoading,
    isInitialSyncComplete,
    error,
    loadMoreForStatus,
  } = useProjectTasksContext();

  // Shared tasks query (Electric SQL)
  const sharedTasksQuery = useLiveQuery(
    useCallback(
      (q) => {
        if (!remoteProjectId || !isSignedIn) {
          return undefined;
        }
        return q
          .from({ sharedTasks: sharedTasksCollection })
          .where(({ sharedTasks }) =>
            eq(sharedTasks.project_id, remoteProjectId)
          )
          .where(({ sharedTasks }) => isNull(sharedTasks.deleted_at));
      },
      [remoteProjectId, isSignedIn]
    ),
    [remoteProjectId, isSignedIn]
  );

  const sharedTasksList = useMemo(
    () => sharedTasksQuery.data ?? [],
    [sharedTasksQuery.data]
  );

  const referencedSharedIds = useMemo(
    () =>
      new Set(
        Object.values(tasksById)
          .map((task) => task.shared_task_id)
          .filter((id): id is string => Boolean(id))
      ),
    [tasksById]
  );

  const { assignees } = useAssigneeUserNames({
    projectId: remoteProjectId || undefined,
    sharedTasks: sharedTasksList,
  });

  const sharedTasksById = useMemo(() => {
    if (!sharedTasksList) return {};
    const map: Record<string, SharedTaskRecord> = {};
    const list = Array.isArray(sharedTasksList) ? sharedTasksList : [];
    for (const task of list) {
      const assignee =
        task.assignee_user_id && assignees
          ? assignees.find((a) => a.user_id === task.assignee_user_id)
          : null;
      map[task.id] = {
        ...task,
        status: task.status,
        remote_project_id: task.project_id,
        assignee_first_name: assignee?.first_name ?? null,
        assignee_last_name: assignee?.last_name ?? null,
        assignee_username: assignee?.username ?? null,
      };
    }
    return map;
  }, [sharedTasksList, assignees]);

  const { tasks, tasksByStatus } = useMemo(() => {
    const merged: Record<string, TaskWithAttemptStatus> = { ...tasksById };
    const byStatus: Record<TaskStatus, TaskWithAttemptStatus[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    Object.values(merged).forEach((task) => {
      byStatus[task.status]?.push(task);
    });

    for (const status of ALL_STATUSES) {
      sortTasksForStatus(byStatus[status], status);
    }

    const sorted = Object.values(merged).sort(
      (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
    );

    return { tasks: sorted, tasksByStatus: byStatus };
  }, [tasksById]);

  const sharedOnlyByStatus = useMemo(() => {
    const grouped: Record<TaskStatus, SharedTaskRecord[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
      cancelled: [],
    };

    Object.values(sharedTasksById).forEach((sharedTask) => {
      const hasLocal =
        Boolean(tasksById[sharedTask.id]) ||
        referencedSharedIds.has(sharedTask.id);

      if (hasLocal) {
        return;
      }
      grouped[sharedTask.status]?.push(sharedTask);
    });

    (Object.values(grouped) as SharedTaskRecord[][]).forEach((list) => {
      list.sort(
        (a, b) =>
          new Date(b.created_at as string).getTime() -
          new Date(a.created_at as string).getTime()
      );
    });

    return grouped;
  }, [tasksById, sharedTasksById, referencedSharedIds]);

  // Derive per-status pagination helpers
  const loadMoreByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = () => loadMoreForStatus(status);
        return acc;
      },
      {} as Record<TaskStatus, () => void>
    );
  }, [loadMoreForStatus]);

  const isLoadingMoreByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = paginationByStatus[status].isLoading;
        return acc;
      },
      {} as Record<TaskStatus, boolean>
    );
  }, [paginationByStatus]);

  const hasMoreByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = paginationByStatus[status].hasMore;
        return acc;
      },
      {} as Record<TaskStatus, boolean>
    );
  }, [paginationByStatus]);

  const totalByStatus = useMemo(() => {
    return ALL_STATUSES.reduce(
      (acc, status) => {
        acc[status] = paginationByStatus[status].total;
        return acc;
      },
      {} as Record<TaskStatus, number>
    );
  }, [paginationByStatus]);

  const anyStatusHasMore = useMemo(
    () => ALL_STATUSES.some((status) => paginationByStatus[status].hasMore),
    [paginationByStatus]
  );

  // Auto-link shared tasks assigned to current user
  useAutoLinkSharedTasks({
    sharedTasksById,
    localTasksById: tasksById,
    referencedSharedIds,
    isLoading: isQueriesLoading,
    hasMore: anyStatusHasMore,
    remoteProjectId: project?.remote_project_id || undefined,
    projectId,
  });

  return {
    tasks,
    tasksById,
    tasksByStatus,
    sharedTasksById,
    sharedOnlyByStatus,
    isLoading: isQueriesLoading,
    isInitialSyncComplete,
    error,
    loadMoreByStatus,
    isLoadingMoreByStatus,
    hasMoreByStatus,
    totalByStatus,
  };
};
