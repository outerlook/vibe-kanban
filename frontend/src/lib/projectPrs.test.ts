import { describe, expect, it } from 'vitest';
import {
  buildProjectPrQuery,
  countLoadedProjectPrs,
  mergeProjectPrPages,
} from './projectPrs';
import type { ProjectPrPageResponse, PrUnresolvedCountsResponse } from './api';

function createPage(
  prNumbers: number[],
  cursor: string | null,
  hasMore: boolean
): ProjectPrPageResponse {
  return {
    repos: [
      {
        repo_id: 'repo-1',
        repo_name: 'repo',
        display_name: 'Repo',
        pull_requests: prNumbers.map((prNumber) => ({
          unresolved_count: null,
          number: prNumber,
          title: `PR ${prNumber}`,
          url: `https://example.com/${prNumber}`,
          author: 'octocat',
          head_branch: 'feature-a',
          base_branch: 'main',
          created_at: '2026-01-01T00:00:00Z',
          updated_at: '2026-01-01T00:00:00Z',
        })),
      },
    ],
    page: {
      limit: 25,
      next_cursor: cursor,
      has_more: hasMore,
    },
  };
}

describe('projectPrs helpers', () => {
  it('normalizes paginated filter queries for server requests', () => {
    expect(
      buildProjectPrQuery({
        cursor: 'next-page',
        filters: {
          baseBranch: 'main',
          search: '  fix login  ',
        },
      })
    ).toEqual({
      cursor: 'next-page',
      limit: 25,
      base_branch: 'main',
      search: 'fix login',
    });

    expect(
      buildProjectPrQuery({
        filters: {
          baseBranch: null,
          search: '   ',
        },
      })
    ).toEqual({
      cursor: null,
      limit: 25,
      base_branch: null,
      search: null,
    });
  });

  it('appends pages and only merges unresolved counts for loaded pages', () => {
    const firstPage = createPage([1], 'cursor-2', true);
    const secondPage = createPage([2], null, false);
    const firstCounts: PrUnresolvedCountsResponse = {
      counts: [
        {
          repo_id: 'repo-1',
          pr_number: 1,
          unresolved_count: 3,
        },
      ],
    };

    const merged = mergeProjectPrPages([
      { response: firstPage, counts: firstCounts },
      { response: secondPage },
    ]);

    expect(merged).toBeDefined();
    expect(merged?.repos).toHaveLength(1);
    expect(merged?.repos[0].pull_requests.map((pr) => pr.number)).toEqual([1, 2]);
    expect(merged?.repos[0].pull_requests.map((pr) => pr.unresolved_count)).toEqual([
      3,
      null,
    ]);
    expect(merged?.page).toEqual(secondPage.page);
    expect(countLoadedProjectPrs(merged)).toBe(2);
  });
});
