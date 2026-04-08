import type {
  GetProjectPrsQuery,
  ProjectPrsResponse,
  PrUnresolvedCountsResponse,
  RepoPrs,
} from './api';

export interface ProjectPrFilters {
  baseBranch: string | null;
  search: string;
}

export interface ProjectPrPageData {
  response: ProjectPrsResponse;
  counts?: PrUnresolvedCountsResponse;
}

export const PROJECT_PRS_PAGE_SIZE = 25;

export function buildProjectPrQuery({
  cursor = null,
  limit = PROJECT_PRS_PAGE_SIZE,
  filters,
}: {
  cursor?: string | null;
  limit?: number;
  filters?: ProjectPrFilters;
} = {}): GetProjectPrsQuery {
  const trimmedSearch = filters?.search.trim() ?? '';

  return {
    cursor,
    limit,
    base_branch: filters?.baseBranch ?? null,
    search: trimmedSearch.length > 0 ? trimmedSearch : null,
  };
}

export function mergeProjectPrPages(
  pages: ProjectPrPageData[]
): ProjectPrsResponse | undefined {
  if (pages.length === 0) {
    return undefined;
  }

  const reposById = new Map<string, RepoPrs>();

  for (const page of pages) {
    const countsByRepo = new Map<string, Map<bigint, number>>();

    for (const count of page.counts?.counts ?? []) {
      let repoCounts = countsByRepo.get(count.repo_id);
      if (!repoCounts) {
        repoCounts = new Map();
        countsByRepo.set(count.repo_id, repoCounts);
      }

      repoCounts.set(count.pr_number, count.unresolved_count);
    }

    for (const repo of page.response.repos) {
      let mergedRepo = reposById.get(repo.repo_id);

      if (!mergedRepo) {
        mergedRepo = {
          ...repo,
          pull_requests: [],
        };
        reposById.set(repo.repo_id, mergedRepo);
      }

      const repoCounts = countsByRepo.get(repo.repo_id);

      mergedRepo.pull_requests.push(
        ...repo.pull_requests.map((pr) => ({
          ...pr,
          unresolved_count: repoCounts?.get(pr.number) ?? pr.unresolved_count,
        }))
      );
    }
  }

  return {
    repos: Array.from(reposById.values()),
    page: pages[pages.length - 1].response.page,
  };
}

export function countLoadedProjectPrs(response?: ProjectPrsResponse): number {
  if (!response) {
    return 0;
  }

  return response.repos.reduce(
    (total, repo) => total + repo.pull_requests.length,
    0
  );
}
