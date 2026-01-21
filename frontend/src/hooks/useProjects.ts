import { useCallback, useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { Operation } from 'rfc6902';
import type { Project } from 'shared/types';
import { projectsApi, getApiBaseUrlSync } from '@/lib/api';

export const projectsKeys = {
  all: ['projects'] as const,
  list: () => ['projects', 'list'] as const,
};

export interface UseProjectsResult {
  projects: Project[];
  projectsById: Record<string, Project>;
  isLoading: boolean;
  isConnected: boolean;
  error: Error | null;
}

type WsJsonPatchMsg = { JsonPatch: Operation[] };
type WsFinishedMsg = { finished: boolean };
type WsMsg = WsJsonPatchMsg | WsFinishedMsg;

const PROJECT_PATH_PREFIX = '/projects/';

const decodePointerSegment = (value: string) =>
  value.replace(/~1/g, '/').replace(/~0/g, '~');

export function useProjects(): UseProjectsResult {
  const [projectsById, setProjectsById] = useState<Record<string, Project>>({});
  const [isConnected, setIsConnected] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // Initial fetch using React Query
  const {
    data: initialProjects,
    isLoading: isQueryLoading,
    error: queryError,
  } = useQuery({
    queryKey: projectsKeys.list(),
    queryFn: projectsApi.list,
    staleTime: 30_000, // 30 seconds
    refetchOnMount: false,
    refetchOnWindowFocus: false,
  });

  // Track if we've synced the initial data
  const [syncedInitial, setSyncedInitial] = useState(false);

  // Sync query results to local state
  useEffect(() => {
    if (!initialProjects || syncedInitial) return;

    const byId: Record<string, Project> = {};
    for (const project of initialProjects) {
      byId[project.id] = project;
    }
    setProjectsById(byId);
    setSyncedInitial(true);
    setError(null);
  }, [initialProjects, syncedInitial]);

  // Handle query error
  useEffect(() => {
    if (queryError) {
      setError(queryError instanceof Error ? queryError : new Error('Failed to load projects'));
    }
  }, [queryError]);

  // Apply JSON patches from WebSocket
  const applyProjectPatches = useCallback(
    (patches: Operation[]) => {
      if (!patches.length) return;

      setProjectsById((prev) => {
        let next = prev;

        for (const op of patches) {
          if (!op.path.startsWith(PROJECT_PATH_PREFIX)) continue;

          const rawId = op.path.slice(PROJECT_PATH_PREFIX.length);
          const projectId = decodePointerSegment(rawId);
          if (!projectId) continue;

          if (op.op === 'remove') {
            if (!next[projectId]) continue;
            if (next === prev) next = { ...prev };
            delete next[projectId];
            continue;
          }

          if (op.op !== 'add' && op.op !== 'replace') continue;

          const project = op.value as Project;
          if (!project || typeof project !== 'object' || !project.id) continue;

          if (op.op === 'replace' && !next[project.id]) continue;

          if (next === prev) next = { ...prev };
          next[project.id] = project;
        }

        return next;
      });
      // Note: We don't invalidate React Query here. Local state (projectsById) is the
      // source of truth for real-time updates. The query cache is only used for initial load.
    },
    []
  );

  // WebSocket for live updates
  useEffect(() => {
    let ws: WebSocket | null = null;
    let retryTimer: number | null = null;
    let retryAttempts = 0;
    let closed = false;

    const scheduleReconnect = () => {
      if (retryTimer) return;
      const delay = Math.min(8000, 1000 * Math.pow(2, retryAttempts));
      retryTimer = window.setTimeout(() => {
        retryTimer = null;
        connect();
      }, delay);
    };

    const connect = () => {
      if (closed) return;

      const endpoint = '/api/projects/stream/ws';
      const fullEndpoint = getApiBaseUrlSync() + endpoint;
      const wsEndpoint = fullEndpoint.replace(/^http/, 'ws');
      ws = new WebSocket(wsEndpoint);

      ws.onopen = () => {
        setIsConnected(true);
        retryAttempts = 0;
      };

      ws.onmessage = (event) => {
        try {
          const msg: WsMsg = JSON.parse(event.data);

          if ('JsonPatch' in msg) {
            applyProjectPatches(msg.JsonPatch);
          }

          if ('finished' in msg) {
            ws?.close(1000, 'finished');
          }
        } catch (err) {
          console.error('Failed to process projects stream:', err);
        }
      };

      ws.onerror = () => {
        // Best-effort live updates; rely on reconnects
      };

      ws.onclose = (evt) => {
        setIsConnected(false);
        if (closed) return;
        if (evt?.code === 1000 && evt?.wasClean) return;
        retryAttempts += 1;
        scheduleReconnect();
      };
    };

    connect();

    return () => {
      closed = true;
      if (retryTimer) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
      if (ws) {
        ws.onopen = null;
        ws.onmessage = null;
        ws.onerror = null;
        ws.onclose = null;
        ws.close();
        ws = null;
      }
    };
  }, [applyProjectPatches]);

  // Derive sorted projects list
  const projects = useMemo(() => {
    return Object.values(projectsById).sort(
      (a, b) =>
        new Date(b.created_at as unknown as string).getTime() -
        new Date(a.created_at as unknown as string).getTime()
    );
  }, [projectsById]);

  // Loading state: we're loading if the query is loading AND we haven't synced yet
  const isLoading = isQueryLoading && !syncedInitial;

  return {
    projects,
    projectsById,
    isLoading,
    isConnected,
    error,
  };
}
