import { useCallback, useMemo } from 'react';
import { useJsonPatchWsStream } from './useJsonPatchWsStream';
import { notificationsApi } from '@/lib/api';
import type { Notification } from 'shared/types';

type NotificationsState = {
  notifications: Record<string, Notification>;
};

interface UseNotificationsResult {
  notifications: Notification[];
  notificationsById: Record<string, Notification>;
  unreadCount: number;
  isConnected: boolean;
  error: string | null;
  markRead: (notificationId: string) => Promise<void>;
  markAllRead: () => Promise<void>;
  deleteNotification: (notificationId: string) => Promise<void>;
}

/**
 * Stream notifications via WebSocket (JSON Patch) for real-time updates.
 * Optionally filter by project_id.
 */
export const useNotifications = (
  projectId?: string
): UseNotificationsResult => {
  const endpoint = notificationsApi.getStreamUrl(projectId);

  const initialData = useCallback(
    (): NotificationsState => ({ notifications: {} }),
    []
  );

  const { data, isConnected, error } = useJsonPatchWsStream<NotificationsState>(
    endpoint,
    true,
    initialData
  );

  const notificationsById = useMemo(
    () => data?.notifications ?? {},
    [data?.notifications]
  );
  const notifications = useMemo(
    () =>
      Object.values(notificationsById).sort(
        (a, b) =>
          new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
      ),
    [notificationsById]
  );

  const unreadCount = useMemo(
    () => notifications.filter((n) => !n.is_read).length,
    [notifications]
  );

  const markRead = useCallback(async (notificationId: string) => {
    await notificationsApi.markRead(notificationId);
  }, []);

  const markAllRead = useCallback(async () => {
    await notificationsApi.markAllRead(projectId);
  }, [projectId]);

  const deleteNotification = useCallback(async (notificationId: string) => {
    await notificationsApi.delete(notificationId);
  }, []);

  return {
    notifications,
    notificationsById,
    unreadCount,
    isConnected,
    error,
    markRead,
    markAllRead,
    deleteNotification,
  };
};
