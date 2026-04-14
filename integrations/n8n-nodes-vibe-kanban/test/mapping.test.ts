import { describe, expect, it } from 'vitest';

import {
  normalizeActionOutput,
  normalizeReadOutput,
} from '../nodes/VibeKanban/shared/output';

describe('node response mapping', () => {
  it('keeps orchestration read payloads expression-friendly', () => {
    const output = normalizeReadOutput('task', {
      task: { id: 'task-1', title: 'Ship it' },
      images: [],
      latest_workspace: null,
      latest_session: null,
      latest_coding_execution: null,
      current_execution_visibility: null,
      pending_tool_approvals: [],
      pending_questions: [],
      dependency_context: {
        blocked_by: [],
        blocking: [],
        ready_dependents: [],
      },
      latest_review_attention: null,
      latest_feedback: null,
      queue_state: {
        execution_queue: null,
        merge_queue: null,
      },
    } as never);

    expect(output.resource).toBe('task');
    expect(output.task.id).toBe('task-1');
    expect(output._meta.surface).toBe('orchestration-context');
  });

  it('normalizes action outputs without hiding the VK DTOs', () => {
    const output = normalizeActionOutput({
      resource: 'approval',
      operation: 'answerApproval',
      identifiers: {
        approvalId: 'approval-1',
        executionProcessId: 'exec-1',
      },
      data: { status: 'approved' } as never,
    });

    expect(output).toEqual({
      resource: 'approval',
      operation: 'answerApproval',
      approvalId: 'approval-1',
      executionProcessId: 'exec-1',
      approvalStatus: { status: 'approved' },
    });
  });
});
