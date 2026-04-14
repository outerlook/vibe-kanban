import { describe, expect, it } from 'vitest';
import {
  getAssociationAtScope,
  getWorkflowAssociationFormError,
  getWorkflowAssociationScopeLabel,
  workflowAssociationFormToPayload,
} from './workflowAssociationHelpers';
import type { WorkflowAssociationResolution } from 'shared/types';

describe('workflowAssociationHelpers', () => {
  it('validates partial workflow form values', () => {
    expect(
      getWorkflowAssociationFormError({
        workflow_id: 'wf-123',
        label: '',
        url: '',
      })
    ).toContain('Enter the workflow ID');

    expect(
      workflowAssociationFormToPayload({
        workflow_id: ' wf-123 ',
        label: ' Review attention ',
        url: 'https://n8n.example/workflow',
      })
    ).toEqual({
      workflow_id: 'wf-123',
      label: 'Review attention',
      url: 'https://n8n.example/workflow',
    });
  });

  it('finds the association at a given scope and formats labels', () => {
    const resolution: WorkflowAssociationResolution = {
      effective: {
        scope: 'task_group_default',
        workflow_id: 'wf-group',
        label: 'Group workflow',
        url: 'https://n8n.example/group',
        is_effective: true,
      },
      associations: [
        {
          scope: 'task_group_default',
          workflow_id: 'wf-group',
          label: 'Group workflow',
          url: 'https://n8n.example/group',
          is_effective: true,
        },
        {
          scope: 'repository_default',
          workflow_id: 'wf-repo',
          label: 'Repo workflow',
          url: 'https://n8n.example/repo',
          is_effective: false,
        },
      ],
    };

    expect(getAssociationAtScope(resolution, 'task_group_default')?.workflow_id).toBe(
      'wf-group'
    );
    expect(getWorkflowAssociationScopeLabel('repository_default')).toBe(
      'Repository default'
    );
  });
});
