import { describe, expect, it } from 'vitest';
import { buildPrPanelData } from './prPanelData';
import type { ProjectPrPageResponse } from '@/lib/api';
import type { TaskGroupWithStats, Workspace } from 'shared/types';

const prsResponse: ProjectPrPageResponse = {
  repos: [
    {
      repo_id: 'repo-1',
      repo_name: 'repo',
      display_name: 'Repo',
      pull_requests: [
        {
          unresolved_count: 1,
          number: 1n,
          title: 'First PR',
          url: 'https://example.com/1',
          author: 'octocat',
          head_branch: 'feature-a',
          base_branch: 'main',
          created_at: '2026-01-01T00:00:00Z',
          updated_at: '2026-01-01T00:00:00Z',
        },
        {
          unresolved_count: 2,
          number: 2n,
          title: 'Second PR',
          url: 'https://example.com/2',
          author: 'octocat',
          head_branch: 'feature-a',
          base_branch: 'main',
          created_at: '2026-01-02T00:00:00Z',
          updated_at: '2026-01-02T00:00:00Z',
        },
      ],
    },
  ],
  page: {
    limit: 25,
    next_cursor: null,
    has_more: false,
  },
};

const taskGroups: TaskGroupWithStats[] = [
  {
    id: 'group-1',
    project_id: 'project-1',
    name: 'Feature A',
    description: 'Shipped branch',
    base_branch: 'feature-a',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    task_counts: {
      todo: 1n,
      inprogress: 2n,
      inreview: 0n,
      done: 3n,
      cancelled: 0n,
    },
  },
  {
    id: 'group-2',
    project_id: 'project-1',
    name: 'Unloaded branch',
    description: null,
    base_branch: 'feature-b',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    task_counts: {
      todo: 9n,
      inprogress: 0n,
      inreview: 0n,
      done: 0n,
      cancelled: 0n,
    },
  },
];

const workspaces: Workspace[] = [
  {
    id: 'workspace-1',
    task_id: 'task-1',
    container_ref: null,
    branch: 'feature-a',
    agent_working_dir: null,
    setup_completed_at: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  },
];

describe('buildPrPanelData', () => {
  it('groups appended PRs without pre-seeding unloaded branches', () => {
    const result = buildPrPanelData({
      prsResponse,
      taskGroups,
      workspaces,
      selectedRepoId: 'repo-1',
      selectedPrNumber: '2',
    });

    expect(Array.from(result.groupedByBranch.keys())).toEqual(['feature-a']);
    expect(result.groupedByBranch.get('feature-a')).toHaveLength(2);
    expect(result.branchMetadata.get('feature-a')).toMatchObject({
      repoId: 'repo-1',
      workspaceId: 'workspace-1',
      groupName: 'Feature A',
    });
    expect(result.branchMetadata.has('feature-b')).toBe(false);
    expect(result.selectedPrData?.id).toBe('repo-1-2');
  });
});
