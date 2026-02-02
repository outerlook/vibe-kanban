import { useEffect, useRef, useState } from 'react';
import { useNotifications } from './useNotifications';
import { useUserSystem } from '@/components/ConfigProvider';
import { playSound, getSoundForNotificationType } from '@/lib/soundUtils';

const LOCK_NAME = 'vibe-kanban-notification-sounds';
const MAX_SEEN_NOTIFICATIONS = 100;

interface UseSoundEffectsOptions {
  projectId?: string;
  enabled?: boolean;
}

interface UseSoundEffectsResult {
  isLeader: boolean;
  error: string | null;
}

/**
 * Hook that handles notification sound playback with cross-tab coordination.
 * Uses Web Locks API to elect a single leader tab that plays sounds.
 */
export function useSoundEffects(
  options: UseSoundEffectsOptions = {}
): UseSoundEffectsResult {
  const { projectId, enabled = true } = options;
  const { config } = useUserSystem();
  const { notifications } = useNotifications(projectId);

  const [isLeader, setIsLeader] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seenNotificationIds = useRef<Set<string>>(new Set());
  const abortControllerRef = useRef<AbortController | null>(null);

  // Leader election via Web Locks API
  useEffect(() => {
    if (!enabled) {
      setIsLeader(false);
      return;
    }

    // Check if Web Locks API is available
    if (!navigator.locks) {
      // Fallback: all tabs play sounds (no coordination)
      setIsLeader(true);
      setError(null);
      return;
    }

    const controller = new AbortController();
    abortControllerRef.current = controller;

    // Request exclusive lock - only one tab gets it
    navigator.locks
      .request(LOCK_NAME, { mode: 'exclusive', signal: controller.signal }, () => {
        // We acquired the lock - we're the leader
        setIsLeader(true);
        setError(null);

        // Keep the lock indefinitely by returning a never-resolving promise
        return new Promise<void>(() => {
          // This promise never resolves, keeping the lock until:
          // 1. The tab closes
          // 2. The component unmounts and we abort
        });
      })
      .catch((err) => {
        // AbortError is expected on cleanup
        if (err.name === 'AbortError') {
          return;
        }
        console.error('Failed to acquire notification sound lock:', err);
        setError(err.message);
        // On error, become leader as fallback (better UX than no sounds)
        setIsLeader(true);
      });

    return () => {
      controller.abort();
      abortControllerRef.current = null;
      setIsLeader(false);
    };
  }, [enabled]);

  // Handle sound playback for new notifications
  useEffect(() => {
    if (!enabled || !isLeader || !config?.notifications?.sound_enabled || !config?.notifications?.frontend_sounds_enabled) {
      return;
    }

    const notificationConfig = config.notifications;

    for (const notification of notifications) {
      // Skip already-seen notifications
      if (seenNotificationIds.current.has(notification.id)) {
        continue;
      }

      // Mark as seen (even if we don't play sound)
      seenNotificationIds.current.add(notification.id);

      // Skip read notifications
      if (notification.is_read) {
        continue;
      }

      // Get sound for this notification type (returns null for unsupported types)
      const soundIdentifier = getSoundForNotificationType(
        notification.notification_type,
        notificationConfig
      );

      if (soundIdentifier) {
        playSound(soundIdentifier);
      }
    }

    // Prevent unbounded growth: keep only the most recent IDs
    if (seenNotificationIds.current.size > MAX_SEEN_NOTIFICATIONS) {
      const idsArray = Array.from(seenNotificationIds.current);
      seenNotificationIds.current = new Set(idsArray.slice(-MAX_SEEN_NOTIFICATIONS));
    }
  }, [notifications, enabled, isLeader, config?.notifications]);

  return { isLeader, error };
}
