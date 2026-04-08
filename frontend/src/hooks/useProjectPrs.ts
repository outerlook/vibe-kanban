import { useInfiniteQuery, useQueries } from '@tanstack/react-query';
import { useMemo } from 'react';
import {
  projectsApi,
  ProjectPrsResponse,
  PrUnresolvedCountsResponse,
} from '@/lib/api';
import {
  buildProjectPrQuery,
  countLoadedProjectPrs,
  mergeProjectPrPages,
  PROJECT_PRS_PAGE_SIZE,
  type ProjectPrFilters,
} from '@/lib/projectPrs';

export const prKeys = {
  all: ['pullRequests'] as const,
  project: (projectId: string | undefined) =>
    ['pullRequests', 'project', projectId] as const,
  list: (
    projectId: string | undefined,
    params: {
      base_branch: string | null;
      search: string | null;
      limit: number | null;
    }
  ) =>
    [
      'pullRequests',
      'project',
      projectId,
      'list',
      params.base_branch,
      params.search,
      params.limit,
    ] as const,
  unresolvedCounts: (
    projectId: string | undefined,
    params: {
      cursor: string | null;
      base_branch: string | null;
      search: string | null;
      limit: number | null;
    }
  ) =>
    [
      'pullRequests',
      'project',
      projectId,
      'unresolvedCounts',
      params.cursor,
      params.base_branch,
      params.search,
      params.limit,
    ] as const,
};

type Options = {
  enabled?: boolean;
  refetchInterval?: number | false;
  staleTime?: number;
  limit?: number;
};

export function useProjectPrs(
  projectId?: string,
  filters?: Partial<ProjectPrFilters>,
  opts?: Options
) {
  const enabled = (opts?.enabled ?? true) && !!projectId;
  const baseQuery = useMemo(
    () =>
      buildProjectPrQuery({
        limit: opts?.limit ?? PROJECT_PRS_PAGE_SIZE,
        filters: {
          baseBranch: filters?.baseBranch ?? null,
          search: filters?.search ?? '',
        },
      }),
    [filters?.baseBranch, filters?.search, opts?.limit]
  );

  const prsQuery = useInfiniteQuery<ProjectPrsResponse, Error>({
    queryKey: prKeys.list(projectId, baseQuery),
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      projectsApi.getPullRequests(projectId!, {
        ...baseQuery,
        cursor: pageParam as string | null,
      }),
    getNextPageParam: (lastPage) => lastPage.page.next_cursor,
    enabled,
    staleTime: opts?.staleTime ?? 30_000,
    refetchInterval: opts?.refetchInterval ?? 60_000,
    retry: 2,
  });

  const countsQueries = useQueries({
    queries:
      enabled && prsQuery.data
        ? prsQuery.data.pages.map((page, index) => {
            const cursor =
              (prsQuery.data?.pageParams[index] as string | null) ?? null;
            const pageQuery = {
              ...baseQuery,
              cursor,
            };

            return {
              queryKey: prKeys.unresolvedCounts(projectId, pageQuery),
              queryFn: () =>
                projectsApi.getPullRequestUnresolvedCounts(projectId!, pageQuery),
              enabled: page.repos.some((repo) => repo.pull_requests.length > 0),
              staleTime: opts?.staleTime ?? 30_000,
              refetchInterval: opts?.refetchInterval ?? 60_000,
              retry: 2,
            };
          })
        : [],
  });

  const data = useMemo<ProjectPrsResponse | undefined>(() => {
    const pages = prsQuery.data?.pages;

    if (!pages) {
      return undefined;
    }

    return mergeProjectPrPages(
      pages.map((page, index) => ({
        response: page,
        counts: countsQueries[index]?.data as
          | PrUnresolvedCountsResponse
          | undefined,
      }))
    );
  }, [prsQuery.data, countsQueries]);

  const countsError = countsQueries.find((query) => query.error)?.error;

  return {
    ...prsQuery,
    data,
    error: prsQuery.error ?? countsError ?? null,
    hasMore: prsQuery.hasNextPage ?? false,
    isLoadingMore: prsQuery.isFetchingNextPage,
    loadMore: () => {
      if (prsQuery.hasNextPage && !prsQuery.isFetchingNextPage) {
        void prsQuery.fetchNextPage();
      }
    },
    loadedCount: countLoadedProjectPrs(data),
  };
}
