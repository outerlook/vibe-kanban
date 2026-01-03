import { useEffect, useRef } from 'react';
import { useUnread } from '@/contexts/UnreadContext';
import type { TaskWithAttemptStatus } from 'shared/types';

interface UseUnreadSyncOptions {
  projectId: string;
  tasks: TaskWithAttemptStatus[];
}

/**
 * Syncs unread state for a project.
 * - Clears acknowledgments when tasks leave 'inreview' status
 * - Updates the project's unread count in context
 */
export function useUnreadSync({ projectId, tasks }: UseUnreadSyncOptions): void {
  const { isTaskUnread, updateProjectUnreadCount, clearTaskAcknowledgment } = useUnread();
  const prevTasksRef = useRef<Map<string, string>>(new Map());

  // Track task status changes and clear acknowledgments when tasks leave inreview
  useEffect(() => {
    const prevTasks = prevTasksRef.current;
    const currentTasks = new Map<string, string>();

    for (const task of tasks) {
      currentTasks.set(task.id, task.status);

      const prevStatus = prevTasks.get(task.id);
      // If task was inreview and now is something else, clear acknowledgment
      if (prevStatus === 'inreview' && task.status !== 'inreview') {
        clearTaskAcknowledgment(task.id);
      }
    }

    prevTasksRef.current = currentTasks;
  }, [tasks, clearTaskAcknowledgment]);

  // Update project unread count whenever tasks or acknowledgments change
  useEffect(() => {
    if (!projectId) return;

    const unreadCount = tasks.filter((task) => isTaskUnread(task)).length;
    updateProjectUnreadCount(projectId, unreadCount);
  }, [projectId, tasks, isTaskUnread, updateProjectUnreadCount]);
}
