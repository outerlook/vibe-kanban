import type { JsonValue } from '../state/types';
import type { DispatchResult, MqttDispatchInput } from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';
import type {
  OrchestrationConversationContextDto,
  OrchestrationExecutionContextDto,
  StructuredOutputContract,
} from '../vk/types';
import {
  createPendingReviewCorrelation,
  getReviewCorrelationByReviewExecution,
  getReviewCorrelationBySource,
  mutateReviewCorrelation,
  reviewAttentionClaimKey,
  reviewCommitMessageClaimKey,
  reviewConversationClaimKey,
  reviewQueueMergeClaimKey,
  REVIEW_GATE_RESULT_WORKFLOW_KEY,
  REVIEW_GATE_SOURCE_WORKFLOW_KEY,
  savePendingReviewCorrelation,
  type ReviewGateCorrelation,
  type ReviewMergeState,
  type ReviewOutcomeState,
} from './review-correlation';

type ReviewDecision = {
  approved: boolean;
  needsAttention: boolean;
  reasoning: string;
};

type ReviewVerdictPayload = {
  needs_attention: boolean;
  reasoning: string;
};

type ReviewGateCandidate = {
  projectId: string;
  taskId: string;
  workspaceId: string;
  taskTitle: string;
  taskDescription: string;
  sourceExecutionProcessId: string;
  worktreePath: string;
  worktreeBranch: string;
  reviewConversationTitle: string;
  reviewInitialMessage: string;
  repoIds: string[];
};

type ClaimOutcome<T extends JsonValue> =
  | { status: 'completed'; result: T; reused: boolean }
  | { status: 'pending' };

type ConversationClaimResult = {
  conversationId: string;
  reviewExecutionProcessId: string;
};

type ReviewAttentionClaimResult = {
  reviewAttentionId: string;
};

type CommitMessageClaimResult = {
  commitMessage: string;
};

type QueueMergeClaimResult = {
  status: 'queued' | 'rejected';
  queueEntryId: string | null;
  queueErrorType: string | null;
};

type ReviewerMessage = OrchestrationConversationContextDto['transcript']['messages'][number];

const REVIEW_VERDICT_STRUCTURED_OUTPUT: StructuredOutputContract = {
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
};

function isJsonRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isReviewVerdictPayload(value: unknown): value is ReviewVerdictPayload {
  if (!isJsonRecord(value)) {
    return false;
  }

  return (
    typeof value.needs_attention === 'boolean' &&
    typeof value.reasoning === 'string' &&
    value.reasoning.trim().length > 0
  );
}

function isReviewStatus(value: unknown): boolean {
  return value === 'inreview' || value === 'in_review';
}

function parseRepoIds(execution: OrchestrationExecutionContextDto): string[] {
  return execution.repo_states
    .map((repoState) => String(repoState.repo_id).trim())
    .filter(
      (repoId, index, repoIds) =>
        repoId.length > 0 && repoIds.indexOf(repoId) === index,
    );
}

function buildReviewInitialMessage(input: {
  taskTitle: string;
  taskDescription: string;
  agentSummary: string;
}): string {
  const description = input.taskDescription || '(no description)';
  const agentSummary = input.agentSummary || '(no agent summary)';

  return `Analyze whether the completed work successfully addresses the original task.

## Original Task
Title: ${input.taskTitle}
Description: ${description}

## Agent's Work Summary
${agentSummary}

## Your Role
You are reviewing whether the task objective was achieved. Check BOTH:
1. Did the agent report any problems or failures?
2. Does the work actually address what the original task requested?

## NEEDS ATTENTION if ANY of these are true:

### Agent-reported problems:
- Errors or failures that weren't resolved
- Work that is incomplete or partially done
- Blockers encountered
- Tests failing
- Uncertainty about correctness

### Task completion issues:
- The work does NOT address the original task objective
- The agent did something tangential (e.g., answered a question but didn't fix the underlying problem)
- The summary describes work that doesn't match what the task asked for
- The task requested a fix/implementation but the agent only investigated/explained

### Agent communication issues:
- Outstanding questions to the user that weren't answered
- Waiting for user input or clarification
- Dependencies on user decisions that haven't been resolved

## Does NOT need attention if:
- The work directly addresses what the task requested
- The agent completed the actual objective (not just related activities)
- No problems or concerns are mentioned by the agent

## IMPORTANT - Do NOT flag attention for:
- Normal configuration requirements (env vars, parameters to set)
- Suggestions for future improvements or additional testing
- Standard deployment steps
- Your own hypothetical concerns about the code
- Things the agent did NOT mention as problems
- Commit message

## Code Review (READ-ONLY)
If you need to clarify something that isn't clear from the summary, you may review the code, but ONLY under these conditions:
- This should be done ONLY when necessary and when it's not clear
- AVOID doing this if possible
- Everything must be READ-ONLY - you cannot change anything
- You may check git diff, view task history, or examine the codebase to clarify doubts
- Keep this focused and minimal

Key question: Did the agent complete what the task actually asked for, or just do something related?
Return your final verdict using the configured structured response.`;
}

function describeReviewerVerdictIssue(message: ReviewerMessage): string | null {
  if (message.metadata_parse_error) {
    return `Reviewer structured output metadata could not be read: ${message.metadata_parse_error}`;
  }

  const structuredOutput = message.metadata?.structured_output;
  if (!structuredOutput) {
    return 'Reviewer execution completed without structured output metadata.';
  }

  if (structuredOutput.status !== 'valid') {
    const errorMessage = structuredOutput.error?.message?.trim();
    return errorMessage
      ? `Reviewer structured output was invalid: ${errorMessage}`
      : 'Reviewer structured output was invalid.';
  }

  if (!isReviewVerdictPayload(structuredOutput.payload ?? null)) {
    return 'Reviewer structured output payload did not match the expected verdict shape.';
  }

  return null;
}

function resolveReviewerDecision(input: {
  reviewExecutionStatus: string;
  reviewExecutionProcessId: string;
  repoIds: string[];
  conversation: OrchestrationConversationContextDto | null;
}): ReviewDecision {
  if (input.reviewExecutionStatus !== 'completed') {
    return {
      approved: false,
      needsAttention: true,
      reasoning: `Reviewer execution finished with status ${input.reviewExecutionStatus}.`,
    };
  }

  const messages = input.conversation?.transcript.messages ?? [];
  const reviewerMessages = messages.filter(
    (message) =>
      message.role === 'assistant' &&
      (!message.execution_process_id ||
        message.execution_process_id === input.reviewExecutionProcessId),
  );
  const finalReviewerMessage = reviewerMessages.at(-1);

  if (!finalReviewerMessage) {
    return {
      approved: false,
      needsAttention: true,
      reasoning: 'Reviewer execution completed without an assistant verdict.',
    };
  }

  const verdictIssue = describeReviewerVerdictIssue(finalReviewerMessage);
  if (verdictIssue) {
    return {
      approved: false,
      needsAttention: true,
      reasoning: verdictIssue,
    };
  }

  const verdictPayload = finalReviewerMessage.metadata?.structured_output?.payload ?? null;
  if (!isReviewVerdictPayload(verdictPayload)) {
    return {
      approved: false,
      needsAttention: true,
      reasoning: 'Reviewer structured output payload did not match the expected verdict shape.',
    };
  }

  const verdict = verdictPayload;
  const approved = !verdict.needs_attention;
  const reasoning = verdict.reasoning.trim() || 'Reviewer returned no reasoning.';

  if (approved && input.repoIds.length === 0) {
    return {
      approved: false,
      needsAttention: true,
      reasoning:
        'Reviewer approved the work, but no workspace repositories were captured for merge.',
    };
  }

  return {
    approved,
    needsAttention: !approved,
    reasoning,
  };
}

function buildInitialOutcome(
  repoIds: string[],
  decision: ReviewDecision,
): ReviewOutcomeState {
  return {
    approved: decision.approved,
    needsAttention: decision.needsAttention,
    reasoning: decision.reasoning,
    reviewedAt: new Date().toISOString(),
    reviewAttentionId: null,
    reviewAttentionStatus: 'pending',
    merges: repoIds.map((repoId) => ({
      repoId,
      commitMessage: null,
      commitMessageStatus: 'pending',
      queueStatus: 'pending',
      queueEntryId: null,
      queueErrorType: null,
    })),
  };
}

function asConversationClaimResult(value: JsonValue | null): ConversationClaimResult | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const conversationId = value.conversationId;
  const reviewExecutionProcessId = value.reviewExecutionProcessId;
  if (typeof conversationId !== 'string' || typeof reviewExecutionProcessId !== 'string') {
    return null;
  }

  return {
    conversationId,
    reviewExecutionProcessId,
  };
}

function asReviewAttentionClaimResult(
  value: JsonValue | null,
): ReviewAttentionClaimResult | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const reviewAttentionId = value.reviewAttentionId;
  if (typeof reviewAttentionId !== 'string') {
    return null;
  }

  return { reviewAttentionId };
}

function asCommitMessageClaimResult(value: JsonValue | null): CommitMessageClaimResult | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const commitMessage = value.commitMessage;
  if (typeof commitMessage !== 'string' || commitMessage.trim().length === 0) {
    return null;
  }

  return { commitMessage };
}

function asQueueMergeClaimResult(value: JsonValue | null): QueueMergeClaimResult | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const status = value.status;
  const queueEntryId = value.queueEntryId;
  const queueErrorType = value.queueErrorType;

  if (
    (status !== 'queued' && status !== 'rejected') ||
    (queueEntryId !== null && typeof queueEntryId !== 'string') ||
    (queueErrorType !== null && typeof queueErrorType !== 'string')
  ) {
    return null;
  }

  return {
    status,
    queueEntryId,
    queueErrorType,
  };
}

async function runClaimedStep<T extends JsonValue>(input: {
  claimKey: string;
  workflowKey: string;
  scopeKey: string;
  metadata: JsonValue;
  deps: RuntimeDependencies;
  parseResult: (value: JsonValue | null) => T | null;
  run: () => Promise<T>;
}): Promise<ClaimOutcome<T>> {
  const existing = input.deps.stateStore.getClaim(input.claimKey);
  const completedResult = existing ? input.parseResult(existing.result) : null;
  if (existing?.status === 'completed' && completedResult) {
    return {
      status: 'completed',
      result: completedResult,
      reused: true,
    };
  }

  if (existing?.status === 'claimed' || existing?.status === 'dispatched') {
    return { status: 'pending' };
  }

  if (existing?.status === 'failed') {
    input.deps.stateStore.markClaimDispatched(input.claimKey, input.metadata);
  } else {
    const claim = input.deps.stateStore.claimEventId({
      claimKey: input.claimKey,
      workflowKey: input.workflowKey,
      scopeKey: input.scopeKey,
      metadata: input.metadata,
    });

    if (!claim.claimed) {
      const reusedResult = input.parseResult(claim.record.result);
      if (claim.record.status === 'completed' && reusedResult) {
        return {
          status: 'completed',
          result: reusedResult,
          reused: true,
        };
      }

      return { status: 'pending' };
    }

    input.deps.stateStore.markClaimDispatched(input.claimKey, input.metadata);
  }

  try {
    const result = await input.run();
    input.deps.stateStore.markClaimCompleted(input.claimKey, result);
    return {
      status: 'completed',
      result,
      reused: false,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : 'review step failed';
    input.deps.stateStore.markClaimFailed(input.claimKey, message);
    throw error;
  }
}

async function resolveReviewExecutionContext(
  input: MqttDispatchInput,
  deps: RuntimeDependencies,
): Promise<OrchestrationExecutionContextDto | null> {
  if (input.contexts.execution) {
    return input.contexts.execution;
  }

  const reviewExecutionProcessId = input.event.entityIds.executionProcessId;
  if (!reviewExecutionProcessId) {
    return null;
  }

  return deps.vkClient.getExecutionContext(reviewExecutionProcessId);
}

async function buildReviewGateCandidate(
  input: MqttDispatchInput,
  deps: RuntimeDependencies,
): Promise<ReviewGateCandidate | null> {
  const taskContext = input.contexts.task;
  const task = taskContext?.task;
  const workspace = taskContext?.latest_workspace;
  const latestCodingExecution = taskContext?.latest_coding_execution;

  if (!task || !workspace || !latestCodingExecution) {
    return null;
  }

  if (taskContext.queue_state.merge_queue?.id) {
    return null;
  }

  if (
    taskContext.latest_review_attention?.execution_process_id === latestCodingExecution.id
  ) {
    return null;
  }

  const sourceExecution = await deps.vkClient.getExecutionContext(latestCodingExecution.id);
  if (sourceExecution.review_attention?.execution_process_id === latestCodingExecution.id) {
    return null;
  }

  const taskTitle = task.title.trim();
  const taskDescription = task.description ?? '';

  return {
    projectId: task.project_id,
    taskId: task.id,
    workspaceId: workspace.id,
    taskTitle,
    taskDescription,
    sourceExecutionProcessId: latestCodingExecution.id,
    worktreePath: workspace.agent_working_dir ?? '',
    worktreeBranch: workspace.branch ?? '',
    reviewConversationTitle: `Review gate: ${taskTitle}`,
    reviewInitialMessage: buildReviewInitialMessage({
      taskTitle,
      taskDescription,
      agentSummary: sourceExecution.coding_agent_turn?.summary ?? '',
    }),
    repoIds: parseRepoIds(sourceExecution),
  };
}

async function ensureReviewConversation(
  correlation: ReviewGateCorrelation,
  deps: RuntimeDependencies,
): Promise<ReviewGateCorrelation | null> {
  if (
    correlation.state.reviewConversationId &&
    correlation.state.reviewExecutionProcessId
  ) {
    return correlation;
  }

  const claimResult = await runClaimedStep({
    claimKey: reviewConversationClaimKey(correlation.state.sourceExecutionProcessId),
    workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
    scopeKey: correlation.state.sourceExecutionProcessId,
    metadata: {
      step: 'create-conversation',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
    },
    deps,
    parseResult: asConversationClaimResult,
    run: async () => {
      const response = await deps.vkClient.createConversation(
        correlation.state.projectId,
        {
          title: correlation.state.reviewConversationTitle,
          initial_message: correlation.state.reviewInitialMessage,
          structured_output: REVIEW_VERDICT_STRUCTURED_OUTPUT,
          executor_profile_id: null,
          worktree_path: correlation.state.worktreePath || null,
          worktree_branch: correlation.state.worktreeBranch || null,
        },
      );

      return {
        conversationId: response.session.id,
        reviewExecutionProcessId: response.execution_process_id,
      } satisfies ConversationClaimResult;
    },
  });

  if (claimResult.status === 'pending') {
    return null;
  }

  return mutateReviewCorrelation(
    deps.stateStore,
    correlation.state.sourceExecutionProcessId,
    (state) => ({
      ...state,
      reviewConversationId: claimResult.result.conversationId,
      reviewExecutionProcessId: claimResult.result.reviewExecutionProcessId,
      registeredAt: state.registeredAt ?? new Date().toISOString(),
    }),
  );
}

async function ensureReviewAttention(
  correlation: ReviewGateCorrelation,
  deps: RuntimeDependencies,
): Promise<ReviewGateCorrelation> {
  const outcome = correlation.state.outcome;
  if (!outcome) {
    throw new Error('Cannot persist review attention before reviewer outcome exists');
  }

  if (outcome.reviewAttentionStatus === 'written' && outcome.reviewAttentionId) {
    return correlation;
  }

  const sourceExecution = await deps.vkClient.getExecutionContext(
    correlation.state.sourceExecutionProcessId,
  );
  const existingAttention = sourceExecution.review_attention;
  if (
    existingAttention &&
    existingAttention.execution_process_id === correlation.state.sourceExecutionProcessId
  ) {
    return mutateReviewCorrelation(
      deps.stateStore,
      correlation.state.sourceExecutionProcessId,
      (state) => ({
        ...state,
        outcome: state.outcome
          ? {
              ...state.outcome,
              reviewAttentionId: existingAttention.id,
              reviewAttentionStatus: 'written',
            }
          : state.outcome,
      }),
    );
  }

  const claimResult = await runClaimedStep({
    claimKey: reviewAttentionClaimKey(correlation.state.sourceExecutionProcessId),
    workflowKey: REVIEW_GATE_RESULT_WORKFLOW_KEY,
    scopeKey: correlation.state.sourceExecutionProcessId,
    metadata: {
      step: 'create-review-attention',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
    },
    deps,
    parseResult: asReviewAttentionClaimResult,
    run: async () => {
      const attention = await deps.vkClient.createReviewAttention({
        execution_process_id: correlation.state.sourceExecutionProcessId,
        task_id: correlation.state.taskId,
        workspace_id: correlation.state.workspaceId,
        needs_attention: outcome.needsAttention,
        reasoning: outcome.reasoning,
      });

      return {
        reviewAttentionId: attention.id,
      } satisfies ReviewAttentionClaimResult;
    },
  });

  if (claimResult.status === 'pending') {
    return correlation;
  }

  return mutateReviewCorrelation(
    deps.stateStore,
    correlation.state.sourceExecutionProcessId,
    (state) => ({
      ...state,
      outcome: state.outcome
        ? {
            ...state.outcome,
            reviewAttentionId: claimResult.result.reviewAttentionId,
            reviewAttentionStatus: 'written',
          }
        : state.outcome,
    }),
  );
}

async function ensureCommitMessage(
  correlation: ReviewGateCorrelation,
  repoId: string,
  deps: RuntimeDependencies,
): Promise<ReviewGateCorrelation> {
  const merge = correlation.state.outcome?.merges.find((entry) => entry.repoId === repoId);
  if (!merge) {
    return correlation;
  }

  if (merge.commitMessageStatus === 'generated' && merge.commitMessage) {
    return correlation;
  }

  const claimResult = await runClaimedStep({
    claimKey: reviewCommitMessageClaimKey(
      correlation.state.sourceExecutionProcessId,
      repoId,
    ),
    workflowKey: REVIEW_GATE_RESULT_WORKFLOW_KEY,
    scopeKey: `${correlation.state.sourceExecutionProcessId}:${repoId}`,
    metadata: {
      step: 'generate-commit-message',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
      repoId,
    },
    deps,
    parseResult: asCommitMessageClaimResult,
    run: async () => {
      const response = await deps.vkClient.generateCommitMessage(
        correlation.state.workspaceId,
        {
          repo_id: repoId,
          executor_profile_id: null,
        },
      );

      return {
        commitMessage: response.commit_message,
      } satisfies CommitMessageClaimResult;
    },
  });

  if (claimResult.status === 'pending') {
    return correlation;
  }

  return mutateReviewCorrelation(
    deps.stateStore,
    correlation.state.sourceExecutionProcessId,
    (state) => ({
      ...state,
      outcome: state.outcome
        ? {
            ...state.outcome,
            merges: state.outcome.merges.map((currentMerge) =>
              currentMerge.repoId === repoId
                ? {
                    ...currentMerge,
                    commitMessage: claimResult.result.commitMessage,
                    commitMessageStatus: 'generated',
                  }
                : currentMerge,
            ),
          }
        : state.outcome,
    }),
  );
}

async function ensureQueueMerge(
  correlation: ReviewGateCorrelation,
  repoId: string,
  deps: RuntimeDependencies,
): Promise<ReviewGateCorrelation> {
  const merge = correlation.state.outcome?.merges.find((entry) => entry.repoId === repoId);
  if (!merge || !merge.commitMessage) {
    return correlation;
  }

  if (merge.queueStatus === 'queued' || merge.queueStatus === 'rejected') {
    return correlation;
  }

  const claimResult = await runClaimedStep({
    claimKey: reviewQueueMergeClaimKey(
      correlation.state.sourceExecutionProcessId,
      repoId,
    ),
    workflowKey: REVIEW_GATE_RESULT_WORKFLOW_KEY,
    scopeKey: `${correlation.state.workspaceId}:${repoId}`,
    metadata: {
      step: 'queue-merge',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
      workspaceId: correlation.state.workspaceId,
      repoId,
    },
    deps,
    parseResult: asQueueMergeClaimResult,
    run: async () => {
      const queueResult = await deps.vkClient.queueMerge(correlation.state.workspaceId, {
        repo_id: repoId,
        commit_message: merge.commitMessage,
      });

      if (queueResult.status === 'queued') {
        return {
          status: 'queued',
          queueEntryId: queueResult.entry.id,
          queueErrorType: null,
        } satisfies QueueMergeClaimResult;
      }

      return {
        status: 'rejected',
        queueEntryId: null,
        queueErrorType: queueResult.error.type,
      } satisfies QueueMergeClaimResult;
    },
  });

  if (claimResult.status === 'pending') {
    return correlation;
  }

  return mutateReviewCorrelation(
    deps.stateStore,
    correlation.state.sourceExecutionProcessId,
    (state) => ({
      ...state,
      outcome: state.outcome
        ? {
            ...state.outcome,
            merges: state.outcome.merges.map((currentMerge) =>
              currentMerge.repoId === repoId
                ? {
                    ...currentMerge,
                    queueStatus: claimResult.result.status,
                    queueEntryId: claimResult.result.queueEntryId,
                    queueErrorType: claimResult.result.queueErrorType,
                  }
                : currentMerge,
            ),
          }
        : state.outcome,
    }),
  );
}

async function handleReviewGate(
  input: MqttDispatchInput,
  deps: RuntimeDependencies,
): Promise<DispatchResult['output']> {
  const candidate = await buildReviewGateCandidate(input, deps);
  if (!candidate) {
    return {
      reason: 'review_gate_not_applicable',
    };
  }

  let correlation = getReviewCorrelationBySource(
    deps.stateStore,
    candidate.sourceExecutionProcessId,
  );

  if (!correlation) {
    correlation = savePendingReviewCorrelation(
      deps.stateStore,
      createPendingReviewCorrelation(candidate),
    );
  }

  if (correlation.state.outcome) {
    return {
      reason: 'review_already_completed',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
      reviewExecutionProcessId: correlation.state.reviewExecutionProcessId,
    };
  }

  const registered = await ensureReviewConversation(correlation, deps);
  if (!registered) {
    return {
      reason: 'review_registration_in_flight',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
    };
  }

  return {
    sourceExecutionProcessId: registered.state.sourceExecutionProcessId,
    reviewExecutionProcessId: registered.state.reviewExecutionProcessId,
    conversationId: registered.state.reviewConversationId,
    repoIds: registered.state.repoIds,
  };
}

async function resolveConversationContext(
  input: MqttDispatchInput,
  correlation: ReviewGateCorrelation,
  deps: RuntimeDependencies,
): Promise<OrchestrationConversationContextDto | null> {
  if (input.contexts.conversation) {
    return input.contexts.conversation;
  }

  if (!correlation.state.reviewConversationId) {
    return null;
  }

  return deps.vkClient.getConversationContext(correlation.state.reviewConversationId);
}

async function handleReviewerResult(
  input: MqttDispatchInput,
  deps: RuntimeDependencies,
): Promise<DispatchResult['output']> {
  const reviewExecutionProcessId = input.event.entityIds.executionProcessId;
  if (!reviewExecutionProcessId) {
    return {
      reason: 'missing_review_execution_process_id',
    };
  }

  let correlation = getReviewCorrelationByReviewExecution(
    deps.stateStore,
    reviewExecutionProcessId,
  );
  if (!correlation) {
    return {
      reason: 'review_execution_not_registered',
      reviewExecutionProcessId,
    };
  }

  const reviewExecution = await resolveReviewExecutionContext(input, deps);
  const reviewExecutionStatus = reviewExecution?.execution.status ?? '';
  const conversation = await resolveConversationContext(input, correlation, deps);

  if (!correlation.state.outcome) {
    const decision = resolveReviewerDecision({
      reviewExecutionStatus,
      reviewExecutionProcessId,
      repoIds: correlation.state.repoIds,
      conversation,
    });

    correlation = mutateReviewCorrelation(
      deps.stateStore,
      correlation.state.sourceExecutionProcessId,
      (state) => ({
        ...state,
        outcome: buildInitialOutcome(state.repoIds, decision),
      }),
    );
  }

  correlation = await ensureReviewAttention(correlation, deps);

  if (!correlation.state.outcome?.approved) {
    return {
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
      reviewExecutionProcessId,
      approved: false,
      needsAttention: correlation.state.outcome?.needsAttention ?? true,
      reviewAttentionId: correlation.state.outcome?.reviewAttentionId ?? null,
    };
  }

  for (const repoId of correlation.state.repoIds) {
    correlation = await ensureCommitMessage(correlation, repoId, deps);
    correlation = await ensureQueueMerge(correlation, repoId, deps);
  }

  return {
    sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
    reviewExecutionProcessId,
    approved: correlation.state.outcome?.approved ?? false,
    needsAttention: correlation.state.outcome?.needsAttention ?? true,
    reviewAttentionId: correlation.state.outcome?.reviewAttentionId ?? null,
    merges: correlation.state.outcome?.merges ?? [],
  };
}

export const reviewGateWorkflowHandler = {
  workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
  matches(input: MqttDispatchInput) {
    return (
      input.event.eventType === 'task_status_changed' &&
      isReviewStatus((input.event.payload as { status?: unknown }).status)
    );
  },
  run(input: MqttDispatchInput, deps: RuntimeDependencies) {
    return handleReviewGate(input, deps);
  },
};

export const reviewerResultWorkflowHandler = {
  workflowKey: REVIEW_GATE_RESULT_WORKFLOW_KEY,
  matches(input: MqttDispatchInput) {
    return input.event.eventType === 'execution_completed';
  },
  run(input: MqttDispatchInput, deps: RuntimeDependencies) {
    return handleReviewerResult(input, deps);
  },
};

export {
  buildReviewInitialMessage,
  buildReviewGateCandidate,
  handleReviewGate,
  handleReviewerResult,
  resolveReviewerDecision,
};
