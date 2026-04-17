import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type { OrchestrationTriggerItem } from '../../../shared/orchestration-events';

import { createConsoleLogger } from '../src/runtime/dependencies';
import { createSqliteStateStore } from '../src/state/sqlite-state-store';
import { dispatchOrchestrationEvent } from '../src/workflows/registry';
import {
  getReviewCorrelationBySource,
  mutateReviewCorrelation,
} from '../src/workflows/review-correlation';

const tempDirs = [] as string[];

afterEach(() => {
  while (tempDirs.length > 0) {
    const next = tempDirs.pop();
    if (next) {
      rmSync(next, { recursive: true, force: true });
    }
  }
});

function createTempDir(prefix: string): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

class FakeVkRuntimeClient {
  readonly createConversationCalls = [] as Array<{
    projectId: string;
    body: Record<string, unknown>;
  }>;
  readonly createReviewAttentionCalls = [] as Array<Record<string, unknown>>;
  readonly generateCommitMessageCalls = [] as Array<{
    workspaceId: string;
    body: Record<string, unknown>;
  }>;
  readonly queueMergeCalls = [] as Array<{
    workspaceId: string;
    body: Record<string, unknown>;
  }>;
  readonly createFeedbackCalls = [] as Array<Record<string, unknown>>;
  readonly order = [] as string[];

  readonly executionContexts = new Map<string, any>();
  readonly conversationContexts = new Map<string, any>();

  createConversationResult = {
    session: { id: 'conversation-1' },
    execution_process_id: 'review-exec-1',
  };

  async getExecutionContext(executionProcessId: string) {
    const context = this.executionContexts.get(executionProcessId);
    if (!context) {
      throw new Error(`Missing fake execution context for ${executionProcessId}`);
    }

    return context;
  }

  async getConversationContext(conversationId: string) {
    const context = this.conversationContexts.get(conversationId);
    if (!context) {
      throw new Error(`Missing fake conversation context for ${conversationId}`);
    }

    return context;
  }

  async createConversation(projectId: string, body: Record<string, unknown>) {
    this.order.push('createConversation');
    this.createConversationCalls.push({ projectId, body });

    return {
      ...this.createConversationResult,
      initial_message: {
        id: 'message-1',
      },
    };
  }

  async createReviewAttention(body: Record<string, unknown>) {
    this.order.push('createReviewAttention');
    this.createReviewAttentionCalls.push(body);

    const attention = {
      id: 'attention-1',
      execution_process_id: String(body.execution_process_id),
      task_id: String(body.task_id),
      workspace_id: String(body.workspace_id),
      needs_attention: Boolean(body.needs_attention),
      reasoning: typeof body.reasoning === 'string' ? body.reasoning : null,
      analyzed_at: '2026-04-16T12:01:00.000Z',
      created_at: '2026-04-16T12:01:00.000Z',
      updated_at: '2026-04-16T12:01:00.000Z',
    };

    const sourceExecution = this.executionContexts.get(attention.execution_process_id);
    if (sourceExecution) {
      sourceExecution.review_attention = {
        id: attention.id,
        execution_process_id: attention.execution_process_id,
        task_id: attention.task_id,
        workspace_id: attention.workspace_id,
        needs_attention: attention.needs_attention,
        reasoning: attention.reasoning,
        analyzed_at: attention.analyzed_at,
      };
    }

    return attention;
  }

  async generateCommitMessage(workspaceId: string, body: Record<string, unknown>) {
    this.order.push('generateCommitMessage');
    this.generateCommitMessageCalls.push({ workspaceId, body });

    return {
      commit_message: `commit:${String(body.repo_id)}`,
    };
  }

  async queueMerge(workspaceId: string, body: Record<string, unknown>) {
    this.order.push('queueMerge');
    this.queueMergeCalls.push({ workspaceId, body });

    return {
      status: 'queued' as const,
      entry: {
        id: `merge-${String(body.repo_id)}`,
      },
    };
  }

  async createFeedback(body: Record<string, unknown>) {
    this.order.push('createFeedback');
    this.createFeedbackCalls.push(body);

    return {
      id: 'feedback-1',
      task_id: String(body.task_id),
      workspace_id: String(body.workspace_id),
      execution_process_id: String(body.execution_process_id),
      feedback: null,
      collected_at: '2026-04-16T12:02:00.000Z',
    };
  }
}

function createTaskContext(args?: {
  latestReviewAttentionExecutionId?: string | null;
}) {
  return {
    task: {
      id: 'task-1',
      project_id: 'project-1',
      title: 'Ship durable review orchestration',
      description: 'Move the review gate into Trigger',
    },
    images: [],
    latest_workspace: {
      id: 'workspace-1',
      branch: 'feature/review-gate',
      agent_working_dir: '/tmp/worktree',
    },
    latest_session: null,
    latest_coding_execution: {
      id: 'source-exec-1',
      status: 'completed',
    },
    current_execution_visibility: null,
    pending_tool_approvals: [],
    pending_questions: [],
    dependency_context: {
      dependents: [],
      blockers: [],
      blocked_by: [],
    },
    latest_review_attention: args?.latestReviewAttentionExecutionId
      ? {
          id: 'attention-existing',
          execution_process_id: args.latestReviewAttentionExecutionId,
          task_id: 'task-1',
          workspace_id: 'workspace-1',
          needs_attention: false,
          reasoning: null,
          analyzed_at: '2026-04-16T12:00:00.000Z',
        }
      : null,
    latest_feedback: null,
    queue_state: {
      execution_queue: null,
      merge_queue: null,
    },
  } as any;
}

function createSourceExecutionContext() {
  return {
    execution: {
      id: 'source-exec-1',
      status: 'completed',
    },
    scope: {
      task: { id: 'task-1' },
      workspace: { id: 'workspace-1' },
      session: null,
      conversation: null,
    },
    coding_agent_turn: {
      summary: 'Implemented the orchestration cutover and verified the happy path.',
    },
    repo_states: [
      {
        repo_id: 'repo-1',
      },
    ],
    current_execution_visibility: null,
    pending_tool_approvals: [],
    pending_questions: [],
    review_attention: null,
    feedback: null,
  } as any;
}

function createReviewExecutionContext(status = 'completed') {
  return {
    execution: {
      id: 'review-exec-1',
      status,
    },
    scope: {
      task: { id: 'task-1' },
      workspace: { id: 'workspace-1' },
      session: null,
      conversation: { id: 'conversation-1' },
    },
    coding_agent_turn: null,
    repo_states: [],
    current_execution_visibility: null,
    pending_tool_approvals: [],
    pending_questions: [],
    review_attention: null,
    feedback: null,
  } as any;
}

function createStructuredReviewVerdict(
  needsAttention: boolean,
  reasoning: string,
) {
  return {
    structured_output: {
      status: 'valid',
      payload: {
        needs_attention: needsAttention,
        reasoning,
      },
      error: null,
    },
  };
}

function createConversationContext(args?: {
  content?: string;
  metadata?: Record<string, unknown> | null;
  metadataParseError?: string | null;
}) {
  const metadata = args?.metadata ?? null;

  return {
    conversation: {
      id: 'conversation-1',
    },
    transcript: {
      messages: [
        {
          id: 'message-1',
          execution_process_id: 'review-exec-1',
          role: 'assistant',
          content: args?.content ?? 'Structured review verdict',
          metadata,
          metadata_json: metadata,
          metadata_raw: metadata ? JSON.stringify(metadata) : null,
          metadata_parse_error: args?.metadataParseError ?? null,
        },
      ],
      images: [],
    },
    executions: [],
    current_execution_visibility: null,
    latest_agent_session_id: null,
  } as any;
}

function createEvent(args: {
  eventType: OrchestrationTriggerItem['eventType'];
  eventId: string;
  payload: Record<string, unknown>;
  executionProcessId?: string | null;
  conversationId?: string | null;
}) {
  return {
    topic: `vk/orchestration/${args.eventType}`,
    eventId: args.eventId,
    schemaVersion: 'vk_orchestration_v1',
    occurredAt: '2026-04-16T12:00:00.000Z',
    eventType: args.eventType,
    entityIds: {
      taskId: 'task-1',
      workspaceId: 'workspace-1',
      sessionId: args.conversationId ?? null,
      executionProcessId: args.executionProcessId ?? null,
      taskGroupId: null,
    },
    payload: args.payload,
    refs: {
      taskContext: { taskId: 'task-1', projectId: 'project-1' },
      taskGroupContext: null,
      conversationContext: args.conversationId
        ? { conversationId: args.conversationId }
        : null,
      executionContext: args.executionProcessId
        ? { executionProcessId: args.executionProcessId }
        : null,
      approvalContext: null,
      taskSession: null,
    },
  } as OrchestrationTriggerItem;
}

function createDependencies(client: FakeVkRuntimeClient) {
  const tempDir = createTempDir('trigger-review-workflow-');
  tempDirs.push(tempDir);

  return {
    environment: {
      vkApi: {
        baseUrl: 'http://127.0.0.1:9',
        authMode: 'none' as const,
      },
      vkMqtt: {
        brokerUrl: 'mqtt://127.0.0.1:1883',
        topicNamespace: 'vk/orchestration',
      },
      stateDatabasePath: join(tempDir, 'state.sqlite'),
      schemaVersion: 'vk_orchestration_v1',
      codeRabbitPollScopeKey: 'global',
      mqttRouterMode: 'direct' as const,
    },
    stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
    vkClient: client as any,
    logger: createConsoleLogger(),
  };
}

describe('review orchestration workflows', () => {
  it('starts the Trigger review gate once per source execution and persists durable correlation', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());

    const deps = createDependencies(client);
    const input = {
      claim: {
        claimKey: 'evt-task-status-1',
      },
      event: createEvent({
        eventType: 'task_status_changed',
        eventId: 'evt-task-status-1',
        payload: { status: 'inreview' },
      }),
      contexts: {
        task: createTaskContext(),
        taskGroup: null,
        conversation: null,
        execution: null,
        approval: null,
      },
    } as any;

    const first = await dispatchOrchestrationEvent(input, deps as any);
    const second = await dispatchOrchestrationEvent(
      {
        ...input,
        event: createEvent({
          eventType: 'task_status_changed',
          eventId: 'evt-task-status-2',
          payload: { status: 'inreview' },
        }),
      },
      deps as any,
    );

    expect(first.handlerKeys).toEqual([
      'review-gate/in-review',
      'lifecycle/autopilot-continuation',
    ]);
    expect(second.handlerKeys).toEqual([
      'review-gate/in-review',
      'lifecycle/autopilot-continuation',
    ]);
    expect(client.createConversationCalls).toHaveLength(1);
    expect(client.createConversationCalls[0]?.body.initial_message).not.toContain(
      'Respond with JSON',
    );
    expect(client.createConversationCalls[0]?.body.initial_message).not.toContain(
      '```json',
    );
    expect(client.createConversationCalls[0]?.body.structured_output).toEqual({
      schema: {
        type: 'object',
        properties: {
          needs_attention: {
            type: 'boolean',
          },
          reasoning: {
            type: 'string',
            minLength: 1,
          },
        },
        required: ['needs_attention', 'reasoning'],
        additionalProperties: false,
      },
    });

    const correlation = getReviewCorrelationBySource(
      deps.stateStore,
      'source-exec-1',
    );
    expect(correlation?.state.reviewConversationId).toBe('conversation-1');
    expect(correlation?.state.reviewExecutionProcessId).toBe('review-exec-1');
    expect(correlation?.state.repoIds).toEqual(['repo-1']);

    deps.stateStore.close();
  });

  it('applies reviewer results once, preserves ordering, and reuses VK review-attention state on replay', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());
    client.executionContexts.set('review-exec-1', createReviewExecutionContext());
    client.conversationContexts.set(
      'conversation-1',
      createConversationContext({
        content: 'The task objective was completed.',
        metadata: createStructuredReviewVerdict(
          false,
          'The task objective was completed.',
        ),
      }),
    );

    const deps = createDependencies(client);

    await dispatchOrchestrationEvent(
      {
        claim: { claimKey: 'evt-task-status-1' },
        event: createEvent({
          eventType: 'task_status_changed',
          eventId: 'evt-task-status-1',
          payload: { status: 'inreview' },
        }),
        contexts: {
          task: createTaskContext(),
          taskGroup: null,
          conversation: null,
          execution: null,
          approval: null,
        },
      } as any,
      deps as any,
    );

    const completionInput = {
      claim: { claimKey: 'evt-review-complete-1' },
      event: createEvent({
        eventType: 'execution_completed',
        eventId: 'evt-review-complete-1',
        payload: {
          status: 'completed',
          run_reason: 'conversation',
          conversation_session_id: 'conversation-1',
        },
        executionProcessId: 'review-exec-1',
        conversationId: 'conversation-1',
      }),
      contexts: {
        task: null,
        taskGroup: null,
        conversation: client.conversationContexts.get('conversation-1') ?? null,
        execution: client.executionContexts.get('review-exec-1') ?? null,
        approval: null,
      },
    } as any;

    const first = await dispatchOrchestrationEvent(completionInput, deps as any);

    expect(first.handlerKeys).toEqual([
      'review-gate/reviewer-result',
      'lifecycle/feedback-collection',
    ]);
    expect(client.order).toEqual([
      'createConversation',
      'createReviewAttention',
      'generateCommitMessage',
      'queueMerge',
      'createFeedback',
    ]);
    expect(client.createReviewAttentionCalls).toHaveLength(1);
    expect(client.generateCommitMessageCalls).toHaveLength(1);
    expect(client.queueMergeCalls).toHaveLength(1);
    expect(client.createFeedbackCalls).toHaveLength(1);

    mutateReviewCorrelation(deps.stateStore, 'source-exec-1', (state) => ({
      ...state,
      outcome: state.outcome
        ? {
            ...state.outcome,
            reviewAttentionId: null,
            reviewAttentionStatus: 'pending',
          }
        : state.outcome,
    }));

    const replay = await dispatchOrchestrationEvent(
      {
        ...completionInput,
        event: createEvent({
          eventType: 'execution_completed',
          eventId: 'evt-review-complete-2',
          payload: {
            status: 'completed',
            run_reason: 'conversation',
            conversation_session_id: 'conversation-1',
          },
          executionProcessId: 'review-exec-1',
          conversationId: 'conversation-1',
        }),
      },
      deps as any,
    );

    expect(replay.handlerKeys).toEqual([
      'review-gate/reviewer-result',
      'lifecycle/feedback-collection',
    ]);
    expect(client.createReviewAttentionCalls).toHaveLength(1);
    expect(client.generateCommitMessageCalls).toHaveLength(1);
    expect(client.queueMergeCalls).toHaveLength(1);
    expect(client.createFeedbackCalls).toHaveLength(2);

    const correlation = getReviewCorrelationBySource(
      deps.stateStore,
      'source-exec-1',
    );
    expect(correlation?.state.outcome?.reviewAttentionId).toBe('attention-1');
    expect(correlation?.state.outcome?.reviewAttentionStatus).toBe('written');
    expect(correlation?.state.outcome?.merges).toEqual([
      {
        repoId: 'repo-1',
        commitMessage: 'commit:repo-1',
        commitMessageStatus: 'generated',
        queueStatus: 'queued',
        queueEntryId: 'merge-repo-1',
        queueErrorType: null,
      },
    ]);

    deps.stateStore.close();
  });

  it('fails closed when the reviewer only returns raw text without structured verdict metadata', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());
    client.executionContexts.set('review-exec-1', createReviewExecutionContext());
    client.conversationContexts.set(
      'conversation-1',
      createConversationContext({
        content:
          '{"needs_attention": false, "reasoning": "Would have been approved by salvage parsing."}',
      }),
    );

    const deps = createDependencies(client);

    await dispatchOrchestrationEvent(
      {
        claim: { claimKey: 'evt-task-status-1' },
        event: createEvent({
          eventType: 'task_status_changed',
          eventId: 'evt-task-status-1',
          payload: { status: 'inreview' },
        }),
        contexts: {
          task: createTaskContext(),
          taskGroup: null,
          conversation: null,
          execution: null,
          approval: null,
        },
      } as any,
      deps as any,
    );

    const result = await dispatchOrchestrationEvent(
      {
        claim: { claimKey: 'evt-review-complete-1' },
        event: createEvent({
          eventType: 'execution_completed',
          eventId: 'evt-review-complete-1',
          payload: {
            status: 'completed',
            run_reason: 'conversation',
            conversation_session_id: 'conversation-1',
          },
          executionProcessId: 'review-exec-1',
          conversationId: 'conversation-1',
        }),
        contexts: {
          task: null,
          taskGroup: null,
          conversation: client.conversationContexts.get('conversation-1') ?? null,
          execution: client.executionContexts.get('review-exec-1') ?? null,
          approval: null,
        },
      } as any,
      deps as any,
    );

    expect(result.handlerKeys).toEqual([
      'review-gate/reviewer-result',
      'lifecycle/feedback-collection',
    ]);
    expect(client.createReviewAttentionCalls).toHaveLength(1);
    expect(client.generateCommitMessageCalls).toHaveLength(0);
    expect(client.queueMergeCalls).toHaveLength(0);
    expect(client.createFeedbackCalls).toHaveLength(1);

    const correlation = getReviewCorrelationBySource(
      deps.stateStore,
      'source-exec-1',
    );
    expect(correlation?.state.outcome?.approved).toBe(false);
    expect(correlation?.state.outcome?.needsAttention).toBe(true);
    expect(correlation?.state.outcome?.reasoning).toContain(
      'structured output metadata',
    );

    deps.stateStore.close();
  });
});
