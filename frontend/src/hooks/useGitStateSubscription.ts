import { useCallback, useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { getApiBaseUrlSync } from '@/lib/api';

interface GitStateChangedMessage {
  message_type: 'git_state_changed';
  workspace_id: string;
}

/**
 * Subscribes to git state changes via WebSocket and invalidates branchStatus query.
 * When the backend detects file system changes in the git worktree, it sends a
 * "git_state_changed" message, triggering a refetch of branch status.
 */
export function useGitStateSubscription(attemptId: string | undefined): void {
  const queryClient = useQueryClient();
  const wsRef = useRef<WebSocket | null>(null);
  const retryTimerRef = useRef<number | null>(null);
  const retryAttemptsRef = useRef<number>(0);
  const [retryNonce, setRetryNonce] = useState(0);

  const scheduleReconnect = useCallback(() => {
    if (retryTimerRef.current) return; // already scheduled
    // Exponential backoff: 1s, 2s, 4s, 8s (max)
    const attempt = retryAttemptsRef.current;
    const delay = Math.min(8000, 1000 * Math.pow(2, attempt));
    retryTimerRef.current = window.setTimeout(() => {
      retryTimerRef.current = null;
      setRetryNonce((n) => n + 1);
    }, delay);
  }, []);

  useEffect(() => {
    if (!attemptId) {
      // Clean up if no attemptId
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        window.clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      retryAttemptsRef.current = 0;
      return;
    }

    // Build WebSocket URL
    const baseUrl = getApiBaseUrlSync();
    const httpUrl = `${baseUrl}/api/task-attempts/${attemptId}/git-status/ws`;
    const wsUrl = httpUrl.replace(/^http/, 'ws');

    const ws = new WebSocket(wsUrl);

    ws.onopen = () => {
      // Reset backoff on successful connection
      retryAttemptsRef.current = 0;
      if (retryTimerRef.current) {
        window.clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };

    ws.onmessage = (event) => {
      try {
        const msg: GitStateChangedMessage = JSON.parse(event.data);
        if (msg.message_type === 'git_state_changed') {
          // Invalidate the branchStatus query to trigger a refetch
          queryClient.invalidateQueries({ queryKey: ['branchStatus', attemptId] });
        }
      } catch (err) {
        console.error('Failed to parse git state message:', err);
      }
    };

    ws.onerror = () => {
      // Error will be followed by onclose, handle reconnect there
    };

    ws.onclose = (evt) => {
      wsRef.current = null;

      // Don't reconnect on clean close
      if (evt?.code === 1000 && evt?.wasClean) {
        return;
      }

      // Reconnect on unexpected closures
      retryAttemptsRef.current += 1;
      scheduleReconnect();
    };

    wsRef.current = ws;

    return () => {
      if (wsRef.current) {
        const ws = wsRef.current;
        ws.onopen = null;
        ws.onmessage = null;
        ws.onerror = null;
        ws.onclose = null;
        ws.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        window.clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, [attemptId, queryClient, scheduleReconnect, retryNonce]);
}
