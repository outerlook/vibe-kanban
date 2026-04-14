import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WorkflowAssociationResolution } from 'shared/types';
import { WorkflowAssociationDetails } from './WorkflowAssociationDetails';

const { useTaskWorkflowAssociations } = vi.hoisted(() => ({
  useTaskWorkflowAssociations: vi.fn(),
}));

vi.mock('@/hooks/useWorkflowAssociations', () => ({
  useTaskWorkflowAssociations,
}));

describe('WorkflowAssociationDetails', () => {
  beforeEach(() => {
    useTaskWorkflowAssociations.mockReset();
  });

  it('shows the empty state when there are no associations', () => {
    useTaskWorkflowAssociations.mockReturnValue({
      data: { effective: null, associations: [] } satisfies WorkflowAssociationResolution,
    });

    const markup = renderToStaticMarkup(
      <WorkflowAssociationDetails taskId="task-1" />
    );

    expect(markup).toContain('No associated workflows');
  });

  it('renders associated workflow metadata and effective badge', () => {
    useTaskWorkflowAssociations.mockReturnValue({
      data: {
        effective: {
          scope: 'task_group_default',
          workflow_id: 'wf-review',
          label: 'Review attention',
          url: 'https://n8n.example/review',
          is_effective: true,
        },
        associations: [
          {
            scope: 'task_group_default',
            workflow_id: 'wf-review',
            label: 'Review attention',
            url: 'https://n8n.example/review',
            is_effective: true,
          },
          {
            scope: 'repository_default',
            workflow_id: 'wf-merge',
            label: 'Generate and merge',
            url: 'https://n8n.example/merge',
            is_effective: false,
          },
        ],
      } satisfies WorkflowAssociationResolution,
    });

    const markup = renderToStaticMarkup(
      <WorkflowAssociationDetails taskId="task-1" />
    );

    expect(markup).toContain('Associated n8n workflows');
    expect(markup).toContain('Review attention');
    expect(markup).toContain('Generate and merge');
    expect(markup).toContain('Effective');
    expect(markup).toContain('Repository default');
  });
});
