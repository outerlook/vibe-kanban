import type { JsonValue } from '../state/types';
import type { DispatchResult, MqttDispatchInput } from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';
import { TRIGGER_EXECUTOR_OPERATION_KEYS } from '../runtime/executor-mapping';
import type {
  ExecutionHistoryRecap,
  OrchestrationExecutionContextDto,
  ExecutionProcessNormalizedEntryRecord,
} from '../vk/types';
import {
  createPendingReviewCorrelation,
  getReviewCorrelationBySource,
  mutateReviewCorrelation,
  reviewAttentionClaimKey,
  reviewCommitMessageClaimKey,
  reviewQueueMergeClaimKey,
  reviewVerdictClaimKey,
  REVIEW_GATE_SOURCE_WORKFLOW_KEY,
  savePendingReviewCorrelation,
  type ReviewGateCorrelation,
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
  taskId: string;
  workspaceId: string;
  taskTitle: string;
  taskDescription: string;
  sourceExecutionProcessId: string;
  sourceExecution: OrchestrationExecutionContextDto;
  worktreePath: string;
  reviewPrompt: string;
  repoIds: string[];
  hasExistingReviewAttention: boolean;
};

type ClaimOutcome<T extends JsonValue> =
  | { status: 'completed'; result: T; reused: boolean }
  | { status: 'pending' };

type ReviewVerdictClaimResult = ReviewDecision;

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

const REVIEW_VERDICT_RESPONSE_SCHEMA = {
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
} satisfies JsonValue;

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
  executionRecap: string;
}): string {
  const description = input.taskDescription || '(no description)';
  const executionRecap = input.executionRecap || '(no execution recap available)';

  return `Analyze whether the completed work successfully addresses the original task.

## Original Task
Title: ${input.taskTitle}
Description: ${description}

## Execution Recap
${executionRecap}

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
- The recap describes work that doesn't match what the task asked for
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
If you need to clarify something that isn't clear from this recap, you may review the code, but ONLY under these conditions:
- This should be done ONLY when necessary and when it's not clear
- AVOID doing this if possible
- Everything must be READ-ONLY - you cannot change anything
- You may check git diff, view task history, or examine the codebase to clarify doubts
- Keep this focused and minimal

Key question: Did the agent complete what the task actually asked for, or just do something related?
Return your final verdict using the configured structured response.`;
}

function buildCommitMessagePrompt(input: {
  taskTitle: string;
  taskDescription: string;
  repoId: string;
  sourceExecution: OrchestrationExecutionContextDto;
}): string {
  const description = input.taskDescription || 'No description provided';
  const repoState = input.sourceExecution.repo_states.find(
    (entry) => String(entry.repo_id) === input.repoId,
  );
  const workspaceBranch = input.sourceExecution.scope.workspace?.branch?.trim() || null;
  const commitRange =
    repoState?.before_head_commit && repoState.after_head_commit
      ? `${repoState.before_head_commit}..${repoState.after_head_commit}`
      : null;

  return [
    'Generate a concise git commit message for the following changes.',
    '',
    `Task: ${input.taskTitle}`,
    `Description: ${description}`,
    `Repo ID: ${input.repoId}`,
    ...(workspaceBranch ? [`Workspace branch: ${workspaceBranch}`] : []),
    ...(commitRange ? [`Commit range: ${commitRange}`] : []),
    '',
    'Inspect the local git changes in READ-ONLY mode before writing the message.',
    'Use the commit range when available; otherwise inspect the current diff/history for this repo.',
    '',
    'Write a commit message following these guidelines:',
    '- First line: imperative mood summary (50 chars max)',
    '- Blank line',
    '- Body: explain what and why (wrap at 72 chars)',
    '',
    'Respond with ONLY the commit message, no other text.',
  ].join('\n');
}

function formatExecutionRecapContent(content: string): string | null {
  const trimmed = content.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function formatExecutionRecapRecord(
  record: ExecutionProcessNormalizedEntryRecord,
  recapIndex: number,
): string | null {
  const content = formatExecutionRecapContent(record.entry.content);
  const type = record.entry.entry_type.type;

  if (type === 'user_message') {
    return `[${recapIndex}] User\n${content ?? '(no content)'}`;
  }

  if (type === 'assistant_message') {
    return `[${recapIndex}] Assistant\n${content ?? '(no content)'}`;
  }

  if (type === 'error_message') {
    const fallbackContent =
      record.entry.entry_type.error_type.type === 'setup_required'
        ? 'Setup was required before execution could continue.'
        : 'Execution reported an error.';

    return `[${recapIndex}] Execution Signal\n${content ?? fallbackContent}`;
  }

  if (type === 'next_action') {
    if (!record.entry.entry_type.failed && !record.entry.entry_type.needs_setup) {
      return null;
    }

    const fallbackContent = record.entry.entry_type.needs_setup
      ? record.entry.entry_type.setup_help_text?.trim() ||
        'Execution reported that setup was still required.'
      : 'Execution reported a failed next action.';

    return `[${recapIndex}] Execution Signal\n${content ?? fallbackContent}`;
  }

  return null;
}

function buildExecutionRecapFromHistory(history: ExecutionHistoryRecap): string | null {
  const recapEntries = [] as string[];

  for (const record of history.entries) {
    const entry = formatExecutionRecapRecord(record, recapEntries.length + 1);
    if (entry) {
      recapEntries.push(entry);
    }
  }

  if (recapEntries.length === 0) {
    return null;
  }

  const preface = [
    'This block is a filtered, derived recap of selected execution history entries in chronological order. It is not a full transcript.',
  ];

  if (history.truncated) {
    preface.push(
      `Older execution context was dropped by the acquisition recap budget; only the latest ${history.budget.maxEntries} normalized entries were retained (${history.droppedEntries} dropped).`,
    );
  }

  return `${preface.join('\n')}\n\n${recapEntries.join('\n\n')}`;
}

function buildFallbackExecutionRecap(finalSummary: string): string {
  const summary = finalSummary.trim();

  if (summary.length > 0) {
    return [
      "Filtered execution history was unavailable or had no usable reviewer context, so this falls back to the coding agent's final summary snapshot.",
      summary,
    ].join('\n\n');
  }

  return [
    'Filtered execution history was unavailable or had no usable reviewer context, and no final summary snapshot was available.',
    '(no execution recap available)',
  ].join('\n\n');
}

async function resolveExecutionRecap(input: {
  sourceExecutionProcessId: string;
  fallbackSummary: string;
  deps: RuntimeDependencies;
}): Promise<string> {
  try {
    const history = await input.deps.vkClient.getExecutionNormalizedEntriesForRecap(
      input.sourceExecutionProcessId,
    );
    const recap = buildExecutionRecapFromHistory(history);
    if (recap) {
      return recap;
    }
  } catch (error) {
    input.deps.logger.warn('Failed to build filtered execution recap for review gate', {
      sourceExecutionProcessId: input.sourceExecutionProcessId,
      error: error instanceof Error ? error.message : 'unknown error',
    });
  }

  return buildFallbackExecutionRecap(input.fallbackSummary);
}

function resolveReviewerDecision(
  verdict: ReviewVerdictPayload,
  repoIds: string[],
): ReviewDecision {
  const approved = !verdict.needs_attention;
  const reasoning = verdict.reasoning.trim() || 'Reviewer returned no reasoning.';

  if (approved && repoIds.length === 0) {
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

function buildReviewerFailureDecision(error: unknown): ReviewDecision {
  const message = error instanceof Error ? error.message.trim() : String(error).trim();

  return {
    approved: false,
    needsAttention: true,
    reasoning: message.length > 0 ? `Reviewer helper failed: ${message}` : 'Reviewer helper failed.',
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

function asReviewVerdictClaimResult(
  value: JsonValue | null,
): ReviewVerdictClaimResult | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const approved = value.approved;
  const needsAttention = value.needsAttention;
  const reasoning = value.reasoning;

  if (
    typeof approved !== 'boolean' ||
    typeof needsAttention !== 'boolean' ||
    typeof reasoning !== 'string' ||
    reasoning.trim().length === 0
  ) {
    return null;
  }

  return {
    approved,
    needsAttention,
    reasoning,
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

  const sourceExecution = await deps.vkClient.getExecutionContext(latestCodingExecution.id);

  const taskTitle = task.title.trim();
  const taskDescription = task.description ?? '';
  const executionRecap = await resolveExecutionRecap({
    sourceExecutionProcessId: latestCodingExecution.id,
    fallbackSummary: sourceExecution.coding_agent_turn?.summary ?? '',
    deps,
  });

  return {
    taskId: task.id,
    workspaceId: workspace.id,
    taskTitle,
    taskDescription,
    sourceExecutionProcessId: latestCodingExecution.id,
    sourceExecution,
    worktreePath: workspace.agent_working_dir ?? '',
    reviewPrompt: buildReviewInitialMessage({
      taskTitle,
      taskDescription,
      executionRecap,
    }),
    repoIds: parseRepoIds(sourceExecution),
    hasExistingReviewAttention:
      taskContext.latest_review_attention?.execution_process_id === latestCodingExecution.id ||
      sourceExecution.review_attention?.execution_process_id === latestCodingExecution.id,
  };
}

async function ensureReviewerOutcome(
  correlation: ReviewGateCorrelation,
  candidate: ReviewGateCandidate,
  deps: RuntimeDependencies,
): Promise<ReviewGateCorrelation | null> {
  if (correlation.state.outcome) {
    return correlation;
  }

  const claimResult = await runClaimedStep({
    claimKey: reviewVerdictClaimKey(correlation.state.sourceExecutionProcessId),
    workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
    scopeKey: correlation.state.sourceExecutionProcessId,
    metadata: {
      step: 'review-verdict',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
    },
    deps,
    parseResult: asReviewVerdictClaimResult,
    run: async () => {
      try {
        const response = await deps.openClawConversationExecutor.run({
          action: 'run',
          prompt: candidate.reviewPrompt,
          response: {
            kind: 'structured',
            schema: REVIEW_VERDICT_RESPONSE_SCHEMA,
          },
          workingDirectory: correlation.state.worktreePath,
          selection: {
            operationKey: TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
          },
          correlation: {
            workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
            scopeKey: correlation.state.sourceExecutionProcessId,
            correlationKey: `review-gate:reviewer:${correlation.state.sourceExecutionProcessId}`,
            idempotencyKey: reviewVerdictClaimKey(
              correlation.state.sourceExecutionProcessId,
            ),
          },
          timeoutMs: 120_000,
          cleanup: {
            onSuccess: 'delete',
            onError: 'delete',
          },
        });

        if (
          response.response.kind !== 'structured' ||
          !isReviewVerdictPayload(response.response.value)
        ) {
          throw new Error(
            'Reviewer helper returned an unexpected structured response payload.',
          );
        }

        return resolveReviewerDecision(
          response.response.value,
          correlation.state.repoIds,
        );
      } catch (error) {
        return buildReviewerFailureDecision(error);
      }
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
      outcome: buildInitialOutcome(state.repoIds, claimResult.result),
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
    workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
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
  candidate: ReviewGateCandidate,
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
    workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
    scopeKey: `${correlation.state.sourceExecutionProcessId}:${repoId}`,
    metadata: {
      step: 'generate-commit-message',
      sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
      repoId,
    },
    deps,
    parseResult: asCommitMessageClaimResult,
    run: async () => {
      const response = await deps.openClawConversationExecutor.run({
        action: 'run',
        prompt: buildCommitMessagePrompt({
          taskTitle: candidate.taskTitle,
          taskDescription: candidate.taskDescription,
          repoId,
          sourceExecution: candidate.sourceExecution,
        }),
        response: {
          kind: 'text',
        },
        workingDirectory: correlation.state.worktreePath,
        selection: {
          operationKey: TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage,
        },
        correlation: {
          workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
          scopeKey: `${correlation.state.workspaceId}:${repoId}`,
          correlationKey: `review-gate:commit-message:${correlation.state.sourceExecutionProcessId}:${repoId}`,
          idempotencyKey: reviewCommitMessageClaimKey(
            correlation.state.sourceExecutionProcessId,
            repoId,
          ),
        },
        timeoutMs: 120_000,
        cleanup: {
          onSuccess: 'delete',
          onError: 'delete',
        },
      });

      return {
        commitMessage: response.response.text.trim(),
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
    workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
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

function buildReviewGateOutput(correlation: ReviewGateCorrelation): DispatchResult['output'] {
  return {
    sourceExecutionProcessId: correlation.state.sourceExecutionProcessId,
    approved: correlation.state.outcome?.approved ?? false,
    needsAttention: correlation.state.outcome?.needsAttention ?? true,
    reviewAttentionId: correlation.state.outcome?.reviewAttentionId ?? null,
    merges: correlation.state.outcome?.merges ?? [],
  };
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

  if (!correlation && candidate.hasExistingReviewAttention) {
    return {
      reason: 'review_gate_not_applicable',
    };
  }

  if (!correlation) {
    correlation = savePendingReviewCorrelation(
      deps.stateStore,
      createPendingReviewCorrelation({
        taskId: candidate.taskId,
        workspaceId: candidate.workspaceId,
        sourceExecutionProcessId: candidate.sourceExecutionProcessId,
        worktreePath: candidate.worktreePath,
        repoIds: candidate.repoIds,
      }),
    );
  }

  correlation = await ensureReviewerOutcome(correlation, candidate, deps);
  if (!correlation) {
    return {
      reason: 'review_verdict_in_flight',
      sourceExecutionProcessId: candidate.sourceExecutionProcessId,
    };
  }

  correlation = await ensureReviewAttention(correlation, deps);

  if (!correlation.state.outcome?.approved) {
    return buildReviewGateOutput(correlation);
  }

  for (const repoId of correlation.state.repoIds) {
    correlation = await ensureCommitMessage(correlation, candidate, repoId, deps);
    correlation = await ensureQueueMerge(correlation, repoId, deps);
  }

  return buildReviewGateOutput(correlation);
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

export {
  buildReviewInitialMessage,
  buildReviewGateCandidate,
  handleReviewGate,
  resolveReviewerDecision,
};
