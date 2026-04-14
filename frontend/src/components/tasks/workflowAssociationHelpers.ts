import type {
  WorkflowAssociation,
  WorkflowAssociationResolution,
  WorkflowAssociationScope,
} from 'shared/types';

export type WorkflowAssociationFormState = {
  workflow_id: string;
  label: string;
  url: string;
};

export const emptyWorkflowAssociationFormState: WorkflowAssociationFormState = {
  workflow_id: '',
  label: '',
  url: '',
};

export function workflowAssociationToFormState(
  association?: Pick<WorkflowAssociation, 'workflow_id' | 'label' | 'url'> | null
): WorkflowAssociationFormState {
  return {
    workflow_id: association?.workflow_id ?? '',
    label: association?.label ?? '',
    url: association?.url ?? '',
  };
}

export function getWorkflowAssociationScopeLabel(
  scope: WorkflowAssociationScope
): string {
  switch (scope) {
    case 'task_override':
      return 'Task override';
    case 'task_group_default':
      return 'Task group default';
    case 'repository_default':
      return 'Repository default';
    default:
      return scope;
  }
}

export function getWorkflowAssociationFormError(
  value: WorkflowAssociationFormState
): string | null {
  const hasAnyValue = Object.values(value).some((field) => field.trim().length > 0);
  const hasAllValues = Object.values(value).every((field) => field.trim().length > 0);

  if (!hasAnyValue) {
    return null;
  }

  if (!hasAllValues) {
    return 'Enter the workflow ID, label, and URL, or clear all three fields.';
  }

  try {
    new URL(value.url.trim());
  } catch {
    return 'Enter a valid workflow URL.';
  }

  return null;
}

export function hasWorkflowAssociationValue(
  value: WorkflowAssociationFormState
): boolean {
  return Object.values(value).some((field) => field.trim().length > 0);
}

export function workflowAssociationFormToPayload(
  value: WorkflowAssociationFormState
): Pick<WorkflowAssociation, 'workflow_id' | 'label' | 'url'> | null {
  if (!hasWorkflowAssociationValue(value)) {
    return null;
  }

  return {
    workflow_id: value.workflow_id.trim(),
    label: value.label.trim(),
    url: value.url.trim(),
  };
}

export function getAssociationAtScope(
  resolution: WorkflowAssociationResolution | undefined,
  scope: WorkflowAssociationScope
) {
  return resolution?.associations.find((association) => association.scope === scope) ?? null;
}
