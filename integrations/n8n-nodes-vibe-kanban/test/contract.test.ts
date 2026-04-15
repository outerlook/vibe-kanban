import { describe, expect, expectTypeOf, it } from 'vitest';

import {
  ORCHESTRATION_SCHEMA_VERSION,
  parseOrchestrationEvent,
  topicForOrchestrationEvent,
} from '../../../shared/orchestration-events';
import type {
  ExecutionProcess,
  ExecutionProcessRunReason,
  OrchestrationEventEnvelope,
  Project,
} from '../../../shared/types';
import {
  VK_ORCHESTRATION_SCHEMA_VERSION,
} from '../nodes/VibeKanban/shared/constants';
import { parseVkOrchestrationEvent } from '../nodes/VibeKanban/shared/events';

function approvalRequestedEvent() {
  return {
    event_id: 'evt-1',
    schema_version: ORCHESTRATION_SCHEMA_VERSION,
    occurred_at: '2026-04-14T10:00:00Z',
    event_type: 'approval_requested' as const,
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
  };
}

describe('shared orchestration contract', () => {
  it('keeps the repo-level parser aligned with the n8n parser', () => {
    const raw = Buffer.from(JSON.stringify(approvalRequestedEvent()));
    const topic = topicForOrchestrationEvent('vk/orchestration', 'approval_requested');

    expect(parseOrchestrationEvent(raw, topic)).toEqual(
      parseVkOrchestrationEvent(raw, topic, VK_ORCHESTRATION_SCHEMA_VERSION),
    );
  });

  it('rejects topics that do not match the event type', () => {
    const raw = Buffer.from(JSON.stringify(approvalRequestedEvent()));

    expect(() =>
      parseOrchestrationEvent(raw, 'vk/orchestration/task_created'),
    ).toThrow(/does not match event type/);
  });

  it('exposes runtime-accurate JSON scalar types', () => {
    expectTypeOf<Project['created_at']>().toEqualTypeOf<string>();
    expectTypeOf<ExecutionProcess['exit_code']>().toEqualTypeOf<number | null>();
    expectTypeOf<ExecutionProcess['input_tokens']>().toEqualTypeOf<number | null>();
    expectTypeOf<ExecutionProcess['output_tokens']>().toEqualTypeOf<number | null>();
    expectTypeOf<ExecutionProcessRunReason>().toEqualTypeOf<
      | 'setup_script'
      | 'cleanup_script'
      | 'coding_agent'
      | 'dev_server'
      | 'internal_agent'
      | 'disposable_conversation'
    >();
    expectTypeOf<OrchestrationEventEnvelope>().toMatchTypeOf<{
      schema_version: 'vk_orchestration_v1';
    }>();
  });
});
