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
          number: 1,
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
          number: 2,
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
      todo: 1,
      inprogress: 2,
      inreview: 0,
      done: 3,
      cancelled: 0,
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
      todo: 9,
      inprogress: 0,
      inreview: 0,
      done: 0,
      cancelled: 0,
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

  it('accepts task count values from JSON responses without bigint revival', () => {
    const taskGroupsFromJson = [
      {
        ...taskGroups[0],
        task_counts: {
          todo: 1,
          inprogress: 2,
          inreview: 0,
          done: 3,
          cancelled: 0,
        },
      },
    ] as unknown as TaskGroupWithStats[];

    const result = buildPrPanelData({
      prsResponse,
      taskGroups: taskGroupsFromJson,
      workspaces,
      selectedRepoId: 'repo-1',
      selectedPrNumber: '1',
    });

    expect(result.branchMetadata.get('feature-a')?.taskCounts).toEqual({
      todo: 1,
      inprogress: 2,
      inreview: 0,
      done: 3,
      cancelled: 0,
    });
  });
});
