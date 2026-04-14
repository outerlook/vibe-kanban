import { describe, expect, it } from 'vitest';

import navbarSource from './components/layout/Navbar.tsx?raw';
import taskPanelSource from './components/panels/TaskPanel.tsx?raw';
import taskCardSource from './components/tasks/TaskCard.tsx?raw';
import projectTasksContextSource from './contexts/ProjectTasksContext.tsx?raw';
import agentSettingsSource from './pages/settings/AgentSettings.tsx?raw';

describe('workflow cutover audit', () => {
  it('removes the old orchestration config fields from the frontend surfaces', () => {
    expect(navbarSource).not.toContain('autopilot_enabled');
    expect(agentSettingsSource).not.toContain('review_attention_executor_profile');
    expect(agentSettingsSource).not.toContain('review_attention_prompt');
  });

  it('uses workflow association surfaces instead of hook-status UI', () => {
    expect(taskCardSource).toContain('WorkflowAssociationBadge');
    expect(taskCardSource).not.toContain('HookStatusBadge');
    expect(taskPanelSource).toContain('WorkflowAssociationDetails');
    expect(taskPanelSource).toContain('Workflows');
    expect(taskPanelSource).not.toContain('Automation');
    expect(projectTasksContextSource).not.toContain('hookExecutionsByTaskId');
  });
});
