import { useEffect, useState, useRef } from 'react';
import type { ServerLogEntry } from 'shared/types';
import { getApiBaseUrlSync } from '@/lib/api';

interface UseServerLogStreamResult {
  logs: ServerLogEntry[];
  error: string | null;
  isConnected: boolean;
}

export const useServerLogStream = (): UseServerLogStreamResult => {
  const [logs, setLogs] = useState<ServerLogEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState<boolean>(false);
  const wsRef = useRef<WebSocket | null>(null);
  const retryCountRef = useRef<number>(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isIntentionallyClosed = useRef<boolean>(false);

  useEffect(() => {
    const open = () => {
      const endpoint = '/api/server-logs/ws';
      const fullEndpoint = getApiBaseUrlSync() + endpoint;
      const wsEndpoint = fullEndpoint.replace(/^http/, 'ws');
      const ws = new WebSocket(wsEndpoint);
      wsRef.current = ws;
      isIntentionallyClosed.current = false;

      ws.onopen = () => {
        setError(null);
        setIsConnected(true);
        retryCountRef.current = 0;
      };

      ws.onmessage = (event) => {
        try {
          const entry = JSON.parse(event.data) as ServerLogEntry;
          setLogs((prev) => [...prev, entry]);
        } catch (e) {
          console.error('Failed to parse server log entry:', e);
        }
      };

      ws.onerror = () => {
        setError('Connection failed');
        setIsConnected(false);
      };

      ws.onclose = (event) => {
        setIsConnected(false);
        // Only retry if the close was not intentional and not a normal closure
        if (!isIntentionallyClosed.current && event.code !== 1000) {
          const next = retryCountRef.current + 1;
          retryCountRef.current = next;
          if (next <= 6) {
            const delay = Math.min(1500, 250 * 2 ** (next - 1));
            retryTimerRef.current = setTimeout(() => open(), delay);
          }
        }
      };
    };

    open();

    return () => {
      if (wsRef.current) {
        isIntentionallyClosed.current = true;
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, []);

  return { logs, error, isConnected };
};
