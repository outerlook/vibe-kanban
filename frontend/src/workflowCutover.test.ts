import { describe, expect, it } from 'vitest';

import navbarSource from './components/layout/Navbar.tsx?raw';
import taskPanelSource from './components/panels/TaskPanel.tsx?raw';
import taskCardSource from './components/tasks/TaskCard.tsx?raw';
import workflowAssociationDetailsSource from './components/tasks/WorkflowAssociationDetails.tsx?raw';
import workflowAssociationFieldsSource from './components/tasks/WorkflowAssociationFields.tsx?raw';
import taskFormDialogSource from './components/dialogs/tasks/TaskFormDialog.tsx?raw';
import taskGroupFormDialogSource from './components/dialogs/tasks/TaskGroupFormDialog.tsx?raw';
import projectSettingsSource from './pages/settings/ProjectSettings.tsx?raw';
import projectTasksContextSource from './contexts/ProjectTasksContext.tsx?raw';
import agentSettingsSource from './pages/settings/AgentSettings.tsx?raw';
import settingsLocaleSource from './i18n/locales/en/settings.json?raw';

describe('workflow cutover audit', () => {
  it('removes the old orchestration config fields from the frontend surfaces', () => {
    expect(navbarSource).not.toContain('autopilot_enabled');
    expect(agentSettingsSource).not.toContain(
      'review_attention_executor_profile'
    );
    expect(agentSettingsSource).not.toContain('review_attention_prompt');
    expect(settingsLocaleSource).not.toContain('reviewAttention');
  });

  it('uses workflow association surfaces instead of hook-status UI', () => {
    expect(taskCardSource).toContain('WorkflowAssociationBadge');
    expect(taskCardSource).not.toContain('HookStatusBadge');
    expect(taskPanelSource).toContain('WorkflowAssociationDetails');
    expect(taskPanelSource).toContain('Workflows');
    expect(taskPanelSource).not.toContain('Automation');
    expect(projectTasksContextSource).not.toContain('hookExecutionsByTaskId');
  });

  it('keeps workflow association copy orchestration-neutral', () => {
    expect(workflowAssociationDetailsSource).toContain('Associated workflows');
    expect(workflowAssociationFieldsSource).toContain('Workflow ID');
    expect(taskFormDialogSource).toContain(
      'Override inherited workflow metadata for this task.'
    );
    expect(taskGroupFormDialogSource).toContain(
      'Set the default workflow metadata for tasks in this group.'
    );
    expect(projectSettingsSource).toContain(
      'Set the repository-level default workflow metadata used when a task group or task does not override it.'
    );
  });
});
