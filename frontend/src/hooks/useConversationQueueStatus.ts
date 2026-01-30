import { useState, useCallback, useEffect } from 'react';
import { conversationQueueApi } from '@/lib/api';
import type { QueueStatus, QueuedMessage } from 'shared/types';

interface UseConversationQueueStatusResult {
  /** Current queue status */
  queueStatus: QueueStatus;
  /** Whether a message is currently queued */
  isQueued: boolean;
  /** The queued message if any */
  queuedMessage: QueuedMessage | null;
  /** Whether an operation is in progress */
  isLoading: boolean;
  /** Queue a new message */
  queueMessage: (message: string, variant: string | null) => Promise<void>;
  /** Cancel the queued message */
  cancelQueue: () => Promise<void>;
  /** Refresh the queue status from the server */
  refresh: () => Promise<void>;
}

export function useConversationQueueStatus(
  conversationId?: string
): UseConversationQueueStatusResult {
  const [queueStatus, setQueueStatus] = useState<QueueStatus>({
    status: 'empty',
  });
  const [isLoading, setIsLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!conversationId) return;
    try {
      const status = await conversationQueueApi.getStatus(conversationId);
      setQueueStatus(status);
    } catch (e) {
      console.error('Failed to fetch conversation queue status:', e);
    }
  }, [conversationId]);

  const queueMessage = useCallback(
    async (message: string, variant: string | null) => {
      if (!conversationId) return;
      setIsLoading(true);
      try {
        const status = await conversationQueueApi.queue(conversationId, {
          message,
          variant,
        });
        setQueueStatus(status);
      } finally {
        setIsLoading(false);
      }
    },
    [conversationId]
  );

  const cancelQueue = useCallback(async () => {
    if (!conversationId) return;
    setIsLoading(true);
    try {
      const status = await conversationQueueApi.cancel(conversationId);
      setQueueStatus(status);
    } finally {
      setIsLoading(false);
    }
  }, [conversationId]);

  // Fetch initial status when conversationId changes
  useEffect(() => {
    if (conversationId) {
      refresh();
    } else {
      setQueueStatus({ status: 'empty' });
    }
  }, [conversationId, refresh]);

  const isQueued = queueStatus.status === 'queued';
  const queuedMessage = isQueued
    ? (queueStatus as Extract<QueueStatus, { status: 'queued' }>).message
    : null;

  return {
    queueStatus,
    isQueued,
    queuedMessage,
    isLoading,
    queueMessage,
    cancelQueue,
    refresh,
  };
}
