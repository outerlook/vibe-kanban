import { describe, expect, it } from 'vitest';

import { VK_ORCHESTRATION_SCHEMA_VERSION } from '../nodes/VibeKanban/shared/constants';
import { parseVkOrchestrationEvent } from '../nodes/VibeKanban/shared/events';

describe('parseVkOrchestrationEvent', () => {
  it('maps approval refs from the compact MQTT contract', () => {
    const event = parseVkOrchestrationEvent(
      Buffer.from(
        JSON.stringify({
          event_id: 'evt-1',
          schema_version: VK_ORCHESTRATION_SCHEMA_VERSION,
          occurred_at: '2026-04-14T10:00:00Z',
          event_type: 'approval_requested',
          task_id: 'task-1',
          workspace_id: 'workspace-1',
          session_id: 'session-1',
          execution_process_id: 'exec-1',
          task_group_id: null,
          payload: {
            approval_id: 'approval-1',
            kind: 'tool_approval',
            tool_call_id: 'tool-call-1',
            tool_name: 'agent_browser',
            question_count: 0,
          },
        }),
      ),
      'vk/orchestration/approval_requested',
    );

    expect(event.refs.approvalContext).toEqual({ approvalId: 'approval-1' });
    expect(event.refs.executionContext).toEqual({ executionProcessId: 'exec-1' });
    expect(event.refs.taskContext).toEqual({
      taskId: 'task-1',
      projectId: null,
    });
  });

  it('maps conversation follow-up events to the conversation context surface', () => {
    const event = parseVkOrchestrationEvent(
      Buffer.from(
        JSON.stringify({
          event_id: 'evt-2',
          schema_version: VK_ORCHESTRATION_SCHEMA_VERSION,
          occurred_at: '2026-04-14T10:00:00Z',
          event_type: 'follow_up_transition',
          task_id: null,
          workspace_id: null,
          session_id: 'conversation-7',
          execution_process_id: null,
          task_group_id: null,
          payload: {
            state: 'queued',
            scope: 'conversation',
            queue_kind: 'after_current_execution',
            execution_process_id: null,
          },
        }),
      ),
      'vk/orchestration/follow_up_transition',
    );

    expect(event.refs.conversationContext).toEqual({
      conversationId: 'conversation-7',
    });
    expect(event.refs.taskSession).toBeNull();
  });
});
