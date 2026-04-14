import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WorkflowAssociationResolution } from 'shared/types';
import { WorkflowAssociationBadge } from './WorkflowAssociationBadge';

const { useTaskWorkflowAssociations } = vi.hoisted(() => ({
  useTaskWorkflowAssociations: vi.fn(),
}));

vi.mock('@/hooks/useWorkflowAssociations', () => ({
  useTaskWorkflowAssociations,
}));

describe('WorkflowAssociationBadge', () => {
  beforeEach(() => {
    useTaskWorkflowAssociations.mockReset();
  });

  it('renders nothing when no effective workflow is associated', () => {
    useTaskWorkflowAssociations.mockReturnValue({
      data: { effective: null, associations: [] } satisfies WorkflowAssociationResolution,
    });

    expect(renderToStaticMarkup(<WorkflowAssociationBadge taskId="task-1" />)).toBe('');
  });

  it('renders the effective workflow label', () => {
    useTaskWorkflowAssociations.mockReturnValue({
      data: {
        effective: {
          scope: 'task_group_default',
          workflow_id: 'wf-review',
          label: 'Review attention',
          url: 'https://n8n.example/review',
          is_effective: true,
        },
        associations: [],
      } satisfies WorkflowAssociationResolution,
    });

    const markup = renderToStaticMarkup(
      <WorkflowAssociationBadge taskId="task-1" />
    );

    expect(markup).toContain('Review attention');
  });
});
