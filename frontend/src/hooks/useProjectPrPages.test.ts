import { describe, expect, it } from 'vitest';
import { prKeys } from './useProjectPrPages';

describe('useProjectPrPages query keys', () => {
  it('scopes invalidation to a project while keeping each page identity distinct', () => {
    const projectId = 'project-1';
    const projectKey = prKeys.project(projectId);
    const firstPageKey = prKeys.pages(projectId, {
      base_branch: 'main',
      search: 'fix login',
      limit: 25,
    });
    const firstCountsKey = prKeys.pageUnresolvedCounts(projectId, {
      cursor: null,
      base_branch: 'main',
      search: 'fix login',
      limit: 25,
    });
    const secondCountsKey = prKeys.pageUnresolvedCounts(projectId, {
      cursor: 'cursor-2',
      base_branch: 'main',
      search: 'fix login',
      limit: 25,
    });

    expect(firstPageKey.slice(0, projectKey.length)).toEqual(projectKey);
    expect(firstCountsKey.slice(0, projectKey.length)).toEqual(projectKey);
    expect(firstCountsKey).not.toEqual(secondCountsKey);
  });
});
