import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo } from 'react';
import {
  projectsApi,
  ProjectPrsResponse,
  PrUnresolvedCountsResponse,
} from '@/lib/api';

export const prKeys = {
  all: ['pullRequests'] as const,
  byProject: (projectId: string | undefined) =>
    ['pullRequests', 'project', projectId] as const,
  unresolvedCounts: (projectId: string | undefined) =>
    ['pullRequests', 'project', projectId, 'unresolvedCounts'] as const,
};

type Options = {
  enabled?: boolean;
  refetchInterval?: number | false;
  staleTime?: number;
};

export function useProjectPrs(projectId?: string, opts?: Options) {
  const enabled = (opts?.enabled ?? true) && !!projectId;
  const queryClient = useQueryClient();

  // Query 1: Fetch PRs (fast) - returns PRs with null unresolved_count
  const prsQuery = useQuery<ProjectPrsResponse>({
    queryKey: prKeys.byProject(projectId),
    queryFn: () => projectsApi.getPullRequests(projectId!),
    enabled,
    staleTime: opts?.staleTime ?? 30_000,
    refetchInterval: opts?.refetchInterval ?? 60_000,
    retry: 2,
  });

  // Query 2: Fetch unresolved counts (slower) - only after PRs are loaded
  const countsQuery = useQuery<PrUnresolvedCountsResponse>({
    queryKey: prKeys.unresolvedCounts(projectId),
    queryFn: () => projectsApi.getPullRequestUnresolvedCounts(projectId!),
    enabled: enabled && prsQuery.isSuccess,
    staleTime: opts?.staleTime ?? 30_000,
    refetchInterval: opts?.refetchInterval ?? 60_000,
    retry: 2,
  });

  // Invalidate counts when PRs are refetched
  useEffect(() => {
    if (prsQuery.dataUpdatedAt && projectId) {
      queryClient.invalidateQueries({
        queryKey: prKeys.unresolvedCounts(projectId),
      });
    }
  }, [prsQuery.dataUpdatedAt, projectId, queryClient]);

  // Merge PRs with their unresolved counts
  const data = useMemo<ProjectPrsResponse | undefined>(() => {
    if (!prsQuery.data) return undefined;

    // If we don't have counts yet, return PRs as-is (with null counts)
    if (!countsQuery.data) return prsQuery.data;

    // Build a lookup map for counts: repoId -> prNumber -> count
    const countsMap = new Map<string, Map<bigint, number>>();
    for (const count of countsQuery.data.counts) {
      let repoMap = countsMap.get(count.repo_id);
      if (!repoMap) {
        repoMap = new Map();
        countsMap.set(count.repo_id, repoMap);
      }
      repoMap.set(count.pr_number, count.unresolved_count);
    }

    // Merge counts into PRs
    return {
      repos: prsQuery.data.repos.map((repo) => ({
        ...repo,
        pull_requests: repo.pull_requests.map((pr) => {
          const repoMap = countsMap.get(repo.repo_id);
          const count = repoMap?.get(pr.number);
          return {
            ...pr,
            unresolved_count: count ?? pr.unresolved_count,
          };
        }),
      })),
    };
  }, [prsQuery.data, countsQuery.data]);

  return {
    ...prsQuery,
    data,
    // Expose whether counts are still loading separately
    isCountsLoading: countsQuery.isLoading,
  };
}
