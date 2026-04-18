import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type { OrchestrationTriggerItem } from '../../../shared/orchestration-events';
import { BaseCodingAgent } from '../../../shared/types';

import type {
  OpenClawConversationRunRequest,
  OpenClawConversationRunResult,
} from '../src/runtime/contracts';
import { createConsoleLogger } from '../src/runtime/dependencies';
import {
  createTriggerExecutorMapping,
  TRIGGER_EXECUTOR_OPERATION_KEYS,
  type TriggerExecutorMapping,
} from '../src/runtime/executor-mapping';
import {
  createOpenClawSessionConfigResolver,
  createTriggerOpenClawProfileMapping,
  type OpenClawSessionConfigResolver,
} from '../src/runtime/openclaw-profile-mapping';
import { createSqliteStateStore } from '../src/state/sqlite-state-store';
import {
  EXECUTION_HISTORY_RECAP_ENTRY_BUDGET,
  type ExecutionHistoryRecap,
  type ExecutionProcessNormalizedEntryRecord,
  type NormalizedEntry,
} from '../src/vk/types';
import { dispatchOrchestrationEvent } from '../src/workflows/registry';
import {
  getReviewCorrelationBySource,
  mutateReviewCorrelation,
} from '../src/workflows/review-correlation';
import {
  createTestOpenClawSessionConfigResolver,
  createTestTriggerExecutorMapping,
} from './helpers';

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
  readonly executionHistoryRecaps = new Map<string, ExecutionHistoryRecap>();

  async getExecutionContext(executionProcessId: string) {
    const context = this.executionContexts.get(executionProcessId);
    if (!context) {
      throw new Error(`Missing fake execution context for ${executionProcessId}`);
    }

    return context;
  }

  async getExecutionNormalizedEntriesForRecap(executionProcessId: string) {
    return (
      this.executionHistoryRecaps.get(executionProcessId) ??
      createExecutionRecap([])
    );
  }

  async createConversation(projectId: string, body: Record<string, unknown>) {
    this.createConversationCalls.push({ projectId, body });
    throw new Error('review workflow should not call VK createConversation');
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
    this.generateCommitMessageCalls.push({ workspaceId, body });
    throw new Error('review workflow should not call VK generateCommitMessage');
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

type OpenClawCall = {
  request: OpenClawConversationRunRequest;
  resolvedSelection: OpenClawConversationRunResult['selection'];
};

class FakeOpenClawConversationExecutor {
  readonly calls = [] as OpenClawCall[];

  reviewerOutcome:
    | { needs_attention: boolean; reasoning: string }
    | Error = {
    needs_attention: false,
    reasoning: 'Looks good to merge.',
  };
  commitMessagesByRepo = new Map<string, string>([['repo-1', 'commit:repo-1']]);

  constructor(
    private readonly environment: {
      triggerExecutorMapping: TriggerExecutorMapping;
      openClawSessionConfigResolver: OpenClawSessionConfigResolver;
    },
    private readonly order: string[],
  ) {}

  async run(
    request: OpenClawConversationRunRequest,
  ): Promise<OpenClawConversationRunResult> {
    const profileName =
      'profileName' in request.selection
        ? request.selection.profileName
        : this.environment.triggerExecutorMapping.resolveProfileName(
            request.selection.operationKey,
          );
    const engineModel = this.environment.openClawSessionConfigResolver.resolve(
      profileName,
    ).engine.model;
    const modelRef = toModelRef(engineModel);

    const resolvedSelection = {
      profileName,
      engineModel,
      modelRef,
    };

    this.calls.push({ request, resolvedSelection });

    if (
      'operationKey' in request.selection &&
      request.selection.operationKey ===
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation
    ) {
      this.order.push('openClaw:reviewer');

      if (this.reviewerOutcome instanceof Error) {
        throw this.reviewerOutcome;
      }

      return {
        action: 'run',
        session: {
          key: 'trigger:openclaw:reviewer',
          id: 'session-reviewer',
          cleanedUp: true,
        },
        selection: resolvedSelection,
        response: {
          kind: 'structured',
          text: JSON.stringify(this.reviewerOutcome),
          value: this.reviewerOutcome,
        },
        run: {
          claimKey: 'claim-reviewer',
          idempotencyKey: request.correlation.idempotencyKey,
          durationMs: 1,
          agentRunId: 'run-reviewer',
          selectedModel: {
            provider: modelRef.split('/')[0] ?? '',
            model: modelRef.split('/').slice(1).join('/'),
          },
        },
      };
    }

    this.order.push('openClaw:commit');
    const repoId = request.correlation.correlationKey.split(':').at(-1) ?? 'repo-1';
    const commitMessage = this.commitMessagesByRepo.get(repoId) ?? `commit:${repoId}`;

    return {
      action: 'run',
      session: {
        key: `trigger:openclaw:${repoId}`,
        id: `session-${repoId}`,
        cleanedUp: true,
      },
      selection: resolvedSelection,
      response: {
        kind: 'text',
        text: commitMessage,
      },
      run: {
        claimKey: `claim-${repoId}`,
        idempotencyKey: request.correlation.idempotencyKey,
        durationMs: 1,
        agentRunId: `run-${repoId}`,
        selectedModel: {
          provider: modelRef.split('/')[0] ?? '',
          model: modelRef.split('/').slice(1).join('/'),
        },
      },
    };
  }
}

function toModelRef(engineModel: string): string {
  if (engineModel.includes('/')) {
    return engineModel;
  }

  const separatorIndex = engineModel.indexOf('.');
  return `${engineModel.slice(0, separatorIndex)}/${engineModel.slice(separatorIndex + 1)}`;
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
      task: {
        id: 'task-1',
        project_id: 'project-1',
        title: 'Ship durable review orchestration',
        description: 'Move the review gate into Trigger',
      },
      workspace: {
        id: 'workspace-1',
        branch: 'feature/review-gate',
      },
      session: null,
      conversation: null,
    },
    coding_agent_turn: {
      summary: 'Implemented the orchestration cutover and verified the happy path.',
    },
    repo_states: [
      {
        repo_id: 'repo-1',
        before_head_commit: 'abc123',
        after_head_commit: 'def456',
      },
    ],
    current_execution_visibility: null,
    pending_tool_approvals: [],
    pending_questions: [],
    review_attention: null,
    feedback: null,
  } as any;
}

function createNormalizedEntryRecord(args: {
  entryIndex: number;
  entryType: NormalizedEntry['entry_type'];
  content: string;
  timestamp?: string | null;
}): ExecutionProcessNormalizedEntryRecord {
  return {
    entry_index: args.entryIndex,
    entry: {
      timestamp: args.timestamp ?? null,
      entry_type: args.entryType,
      content: args.content,
      metadata: null,
    },
  };
}

function createExecutionRecap(
  entries: ExecutionProcessNormalizedEntryRecord[],
  args?: {
    droppedEntries?: number;
  },
): ExecutionHistoryRecap {
  const droppedEntries = args?.droppedEntries ?? 0;

  return {
    entries,
    totalEntries: entries.length + droppedEntries,
    droppedEntries,
    truncated: droppedEntries > 0,
    budget: {
      maxEntries: EXECUTION_HISTORY_RECAP_ENTRY_BUDGET,
      truncation: 'drop_oldest',
    },
  };
}

function createEvent(args: {
  eventType: OrchestrationTriggerItem['eventType'];
  eventId: string;
  payload: Record<string, unknown>;
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
      sessionId: null,
      executionProcessId: null,
      taskGroupId: null,
    },
    payload: args.payload,
    refs: {
      taskContext: { taskId: 'task-1', projectId: 'project-1' },
      taskGroupContext: null,
      conversationContext: null,
      executionContext: null,
      approvalContext: null,
      taskSession: null,
    },
  } as OrchestrationTriggerItem;
}

function createInReviewInput(eventId = 'evt-task-status-1') {
  return {
    claim: {
      claimKey: eventId,
    },
    event: createEvent({
      eventType: 'task_status_changed',
      eventId,
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
}

function createDependencies(args?: {
  client?: FakeVkRuntimeClient;
  triggerExecutorMapping?: TriggerExecutorMapping;
  openClawSessionConfigResolver?: OpenClawSessionConfigResolver;
  reviewerOutcome?: { needs_attention: boolean; reasoning: string } | Error;
  commitMessagesByRepo?: Map<string, string>;
}) {
  const tempDir = createTempDir('trigger-review-workflow-');
  tempDirs.push(tempDir);

  const client = args?.client ?? new FakeVkRuntimeClient();
  const triggerExecutorMapping =
    args?.triggerExecutorMapping ?? createTestTriggerExecutorMapping();
  const openClawSessionConfigResolver =
    args?.openClawSessionConfigResolver ?? createTestOpenClawSessionConfigResolver();

  const environment = {
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
    triggerExecutorMapping,
    openClawSessionConfigResolver,
  };

  const openClawConversationExecutor = new FakeOpenClawConversationExecutor(
    environment,
    client.order,
  );
  if (args?.reviewerOutcome !== undefined) {
    openClawConversationExecutor.reviewerOutcome = args.reviewerOutcome;
  }
  if (args?.commitMessagesByRepo) {
    openClawConversationExecutor.commitMessagesByRepo = args.commitMessagesByRepo;
  }

  return {
    environment,
    stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
    vkClient: client as any,
    logger: createConsoleLogger(),
    openClawConversationExecutor,
    client,
  };
}

describe('review orchestration workflows', () => {
  it('starts the review gate with a chronological execution recap and persists simplified durable correlation', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());
    client.executionHistoryRecaps.set(
      'source-exec-1',
      createExecutionRecap([
        createNormalizedEntryRecord({
          entryIndex: 1,
          entryType: { type: 'user_message' },
          content: 'Please move the review gate into Trigger.',
        }),
        createNormalizedEntryRecord({
          entryIndex: 2,
          entryType: {
            type: 'tool_use',
            tool_name: 'rg',
            action_type: {
              action: 'search',
              query: 'review gate',
            },
            status: {
              status: 'success',
            },
          },
          content: 'Searched for the existing review flow.',
        }),
        createNormalizedEntryRecord({
          entryIndex: 3,
          entryType: { type: 'assistant_message' },
          content: 'I moved the review gate orchestration into Trigger runtime.',
        }),
        createNormalizedEntryRecord({
          entryIndex: 4,
          entryType: { type: 'assistant_message' },
          content: 'I also verified the happy path and updated the tests.',
        }),
      ]),
    );

    const deps = createDependencies({ client });

    const first = await dispatchOrchestrationEvent(
      createInReviewInput('evt-task-status-1'),
      deps as any,
    );
    const second = await dispatchOrchestrationEvent(
      createInReviewInput('evt-task-status-2'),
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

    const reviewCall = deps.openClawConversationExecutor.calls.find(
      (call) =>
        'operationKey' in call.request.selection &&
        call.request.selection.operationKey ===
          TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
    );
    expect(reviewCall).toBeDefined();
    expect(reviewCall?.request.prompt).toContain('## Execution Recap');
    expect(reviewCall?.request.prompt).toContain(
      'filtered, derived recap of selected execution history entries in chronological order',
    );
    expect(reviewCall?.request.prompt).not.toContain("Agent's Work Summary");
    expect(reviewCall?.request.response).toEqual({
      kind: 'structured',
      schema: {
        type: 'object',
        properties: {
          needs_attention: { type: 'boolean' },
          reasoning: { type: 'string', minLength: 1 },
        },
        required: ['needs_attention', 'reasoning'],
        additionalProperties: false,
      },
    });

    expect(client.createConversationCalls).toHaveLength(0);
    expect(client.generateCommitMessageCalls).toHaveLength(0);

    const correlation = getReviewCorrelationBySource(
      deps.stateStore,
      'source-exec-1',
    );
    expect(correlation?.state.taskId).toBe('task-1');
    expect(correlation?.state.workspaceId).toBe('workspace-1');
    expect(correlation?.state.worktreePath).toBe('/tmp/worktree');
    expect(correlation?.state.repoIds).toEqual(['repo-1']);
    expect(Object.hasOwn(correlation?.state ?? {}, 'reviewConversationId')).toBe(false);
    expect(Object.hasOwn(correlation?.state ?? {}, 'reviewExecutionProcessId')).toBe(false);

    deps.stateStore.close();
  });

  it('falls back to the coding agent summary when filtered history is unavailable', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());

    const deps = createDependencies({ client });

    await dispatchOrchestrationEvent(createInReviewInput(), deps as any);

    const reviewCall = deps.openClawConversationExecutor.calls[0];
    expect(reviewCall?.request.prompt).toContain(
      "falls back to the coding agent's final summary snapshot",
    );
    expect(reviewCall?.request.prompt).toContain(
      'Implemented the orchestration cutover and verified the happy path.',
    );

    deps.stateStore.close();
  });

  it('surfaces acquisition truncation in the execution recap prompt block', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());
    client.executionHistoryRecaps.set(
      'source-exec-1',
      createExecutionRecap(
        [
          createNormalizedEntryRecord({
            entryIndex: 401,
            entryType: { type: 'user_message' },
            content: 'Retained entry one.',
          }),
          createNormalizedEntryRecord({
            entryIndex: 402,
            entryType: { type: 'assistant_message' },
            content: 'Retained entry two.',
          }),
        ],
        { droppedEntries: 17 },
      ),
    );

    const deps = createDependencies({ client });

    await dispatchOrchestrationEvent(createInReviewInput(), deps as any);

    const reviewCall = deps.openClawConversationExecutor.calls[0];
    expect(reviewCall?.request.prompt).toContain(
      `only the latest ${EXECUTION_HISTORY_RECAP_ENTRY_BUDGET} normalized entries were retained (17 dropped)`,
    );
    expect(reviewCall?.request.prompt).toContain('[1] User');
    expect(reviewCall?.request.prompt).toContain('[2] Assistant');

    deps.stateStore.close();
  });

  it('runs reviewer and commit helper work through Trigger-owned OpenClaw execution and repairs replayed state from durable claims', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());

    const deps = createDependencies({ client });

    const first = await dispatchOrchestrationEvent(
      createInReviewInput('evt-task-status-1'),
      deps as any,
    );

    expect(first.handlerKeys).toEqual([
      'review-gate/in-review',
      'lifecycle/autopilot-continuation',
    ]);
    expect(client.order).toEqual([
      'openClaw:reviewer',
      'createReviewAttention',
      'openClaw:commit',
      'queueMerge',
    ]);
    expect(client.createReviewAttentionCalls).toHaveLength(1);
    expect(client.queueMergeCalls).toHaveLength(1);

    mutateReviewCorrelation(deps.stateStore, 'source-exec-1', (state) => ({
      ...state,
      outcome: state.outcome
        ? {
            ...state.outcome,
            reviewAttentionId: null,
            reviewAttentionStatus: 'pending',
            merges: state.outcome.merges.map((merge) => ({
              ...merge,
              queueStatus: 'pending',
              queueEntryId: null,
              queueErrorType: null,
            })),
          }
        : state.outcome,
    }));

    const replay = await dispatchOrchestrationEvent(
      createInReviewInput('evt-task-status-2'),
      deps as any,
    );

    expect(replay.handlerKeys).toEqual([
      'review-gate/in-review',
      'lifecycle/autopilot-continuation',
    ]);
    expect(client.order).toEqual([
      'openClaw:reviewer',
      'createReviewAttention',
      'openClaw:commit',
      'queueMerge',
    ]);
    expect(deps.openClawConversationExecutor.calls).toHaveLength(2);
    expect(client.createReviewAttentionCalls).toHaveLength(1);
    expect(client.queueMergeCalls).toHaveLength(1);

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

  it('creates review attention and skips merge work when the reviewer asks for attention', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());

    const deps = createDependencies({
      client,
      reviewerOutcome: {
        needs_attention: true,
        reasoning: 'Tests are still failing.',
      },
    });

    const result = await dispatchOrchestrationEvent(
      createInReviewInput(),
      deps as any,
    );

    expect(result.handlerKeys).toEqual([
      'review-gate/in-review',
      'lifecycle/autopilot-continuation',
    ]);
    expect(client.createReviewAttentionCalls).toHaveLength(1);
    expect(client.queueMergeCalls).toHaveLength(0);
    expect(deps.openClawConversationExecutor.calls).toHaveLength(1);
    expect((result.output as any)[0]).toMatchObject({
      approved: false,
      needsAttention: true,
      reviewAttentionId: 'attention-1',
    });

    const correlation = getReviewCorrelationBySource(
      deps.stateStore,
      'source-exec-1',
    );
    expect(correlation?.state.outcome?.reasoning).toBe('Tests are still failing.');

    deps.stateStore.close();
  });

  it('resolves reviewer and commit helpers through mapped engine.model selection without code changes', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());

    const triggerExecutorMapping = createTriggerExecutorMapping(
      {
        [TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution]: {
          executor: BaseCodingAgent.CODEX,
          variant: 'DEFAULT',
        },
        [TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation]: {
          executor: BaseCodingAgent.CLAUDE_CODE,
          variant: 'REVIEWER_SPECIALIST',
        },
        [TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage]: {
          executor: BaseCodingAgent.CODEX,
          variant: 'COMMIT_COMPOSER',
        },
      },
      '<test-distinct-review-executors>',
    );
    const openClawSessionConfigResolver = createOpenClawSessionConfigResolver(
      createTriggerOpenClawProfileMapping(
        {
          'CODEX.DEFAULT': 'openai.gpt-5',
          'CLAUDE_CODE.REVIEWER_SPECIALIST': 'anthropic.claude-sonnet-4-5',
          'CODEX.COMMIT_COMPOSER': 'openai.gpt-5-mini',
        },
        '<test-openclaw-profile-map>',
      ),
    );
    const deps = createDependencies({
      client,
      triggerExecutorMapping,
      openClawSessionConfigResolver,
    });

    await dispatchOrchestrationEvent(createInReviewInput(), deps as any);

    const reviewerCall = deps.openClawConversationExecutor.calls.find(
      (call) =>
        'operationKey' in call.request.selection &&
        call.request.selection.operationKey ===
          TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
    );
    const commitCall = deps.openClawConversationExecutor.calls.find(
      (call) =>
        'operationKey' in call.request.selection &&
        call.request.selection.operationKey ===
          TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage,
    );

    expect(reviewerCall?.resolvedSelection).toEqual({
      profileName: 'CLAUDE_CODE.REVIEWER_SPECIALIST',
      engineModel: 'anthropic.claude-sonnet-4-5',
      modelRef: 'anthropic/claude-sonnet-4-5',
    });
    expect(commitCall?.resolvedSelection).toEqual({
      profileName: 'CODEX.COMMIT_COMPOSER',
      engineModel: 'openai.gpt-5-mini',
      modelRef: 'openai/gpt-5-mini',
    });

    deps.stateStore.close();
  });

  it('fails closed when the reviewer helper errors instead of returning a verdict', async () => {
    const client = new FakeVkRuntimeClient();
    client.executionContexts.set('source-exec-1', createSourceExecutionContext());

    const deps = createDependencies({
      client,
      reviewerOutcome: new Error('OpenClaw structured response did not match the requested schema.'),
    });

    await dispatchOrchestrationEvent(createInReviewInput(), deps as any);

    expect(client.createReviewAttentionCalls).toHaveLength(1);
    expect(client.queueMergeCalls).toHaveLength(0);
    expect(client.generateCommitMessageCalls).toHaveLength(0);

    const correlation = getReviewCorrelationBySource(
      deps.stateStore,
      'source-exec-1',
    );
    expect(correlation?.state.outcome?.approved).toBe(false);
    expect(correlation?.state.outcome?.needsAttention).toBe(true);
    expect(correlation?.state.outcome?.reasoning).toContain('Reviewer helper failed');
    expect(correlation?.state.outcome?.reasoning).toContain('structured response');

    deps.stateStore.close();
  });
});
