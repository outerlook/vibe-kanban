import type {
  JsonValue,
  OrchestratorStateStore,
  ReviewCorrelationRecord,
} from '../state/types';

export const REVIEW_GATE_SOURCE_WORKFLOW_KEY = 'review-gate/in-review';
export const REVIEW_GATE_RESULT_WORKFLOW_KEY = 'review-gate/reviewer-result';

const REVIEW_GATE_PROVIDER = 'review-gate';

export type ReviewQueueStatus = 'pending' | 'queued' | 'rejected';

export type ReviewMergeState = {
  repoId: string;
  commitMessage: string | null;
  commitMessageStatus: 'pending' | 'generated';
  queueStatus: ReviewQueueStatus;
  queueEntryId: string | null;
  queueErrorType: string | null;
};

export type ReviewOutcomeState = {
  approved: boolean;
  needsAttention: boolean;
  reasoning: string;
  reviewedAt: string;
  reviewAttentionId: string | null;
  reviewAttentionStatus: 'pending' | 'written';
  merges: ReviewMergeState[];
};

export type ReviewGateSourceState = {
  kind: 'review-gate-source';
  sourceExecutionProcessId: string;
  projectId: string;
  taskId: string;
  workspaceId: string;
  worktreePath: string;
  worktreeBranch: string;
  reviewConversationTitle: string;
  reviewInitialMessage: string;
  repoIds: string[];
  reviewConversationId: string | null;
  reviewExecutionProcessId: string | null;
  registeredAt: string | null;
  outcome: ReviewOutcomeState | null;
};

type ReviewGatePointerState = {
  kind: 'review-gate-review-pointer';
  sourceExecutionProcessId: string;
  reviewExecutionProcessId: string;
};

export type ReviewGateCorrelation = {
  record: ReviewCorrelationRecord;
  state: ReviewGateSourceState;
};

type ReviewGateMutation = (
  current: ReviewGateSourceState,
) => ReviewGateSourceState;

function isJsonRecord(
  value: JsonValue | null | undefined,
): value is Record<string, JsonValue> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function asString(value: JsonValue | undefined): string | null {
  return typeof value === 'string' ? value : null;
}

function asStringArray(value: JsonValue | undefined): string[] | null {
  if (!Array.isArray(value)) {
    return null;
  }

  const entries = value
    .map((entry) => (typeof entry === 'string' ? entry : null))
    .filter((entry): entry is string => entry !== null && entry.length > 0);

  return entries.length === value.length ? entries : null;
}

function parseMergeState(value: JsonValue): ReviewMergeState | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const repoId = asString(value.repoId);
  const commitMessage = value.commitMessage === null ? null : asString(value.commitMessage);
  const commitMessageStatus = asString(value.commitMessageStatus);
  const queueStatus = asString(value.queueStatus);
  const queueEntryId = value.queueEntryId === null ? null : asString(value.queueEntryId);
  const queueErrorType =
    value.queueErrorType === null ? null : asString(value.queueErrorType);

  if (
    !repoId ||
    (commitMessage !== null && typeof commitMessage !== 'string') ||
    (commitMessageStatus !== 'pending' && commitMessageStatus !== 'generated') ||
    (queueStatus !== 'pending' && queueStatus !== 'queued' && queueStatus !== 'rejected') ||
    (queueEntryId !== null && typeof queueEntryId !== 'string') ||
    (queueErrorType !== null && typeof queueErrorType !== 'string')
  ) {
    return null;
  }

  return {
    repoId,
    commitMessage,
    commitMessageStatus,
    queueStatus,
    queueEntryId,
    queueErrorType,
  };
}

function parseOutcomeState(value: JsonValue | undefined): ReviewOutcomeState | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const approved = value.approved;
  const needsAttention = value.needsAttention;
  const reasoning = asString(value.reasoning);
  const reviewedAt = asString(value.reviewedAt);
  const reviewAttentionId =
    value.reviewAttentionId === null ? null : asString(value.reviewAttentionId);
  const reviewAttentionStatus = asString(value.reviewAttentionStatus);

  if (
    typeof approved !== 'boolean' ||
    typeof needsAttention !== 'boolean' ||
    !reasoning ||
    !reviewedAt ||
    (reviewAttentionId !== null && typeof reviewAttentionId !== 'string') ||
    (reviewAttentionStatus !== 'pending' && reviewAttentionStatus !== 'written') ||
    !Array.isArray(value.merges)
  ) {
    return null;
  }

  const merges = value.merges
    .map((entry) => parseMergeState(entry))
    .filter((entry): entry is ReviewMergeState => entry !== null);

  if (merges.length !== value.merges.length) {
    return null;
  }

  return {
    approved,
    needsAttention,
    reasoning,
    reviewedAt,
    reviewAttentionId,
    reviewAttentionStatus,
    merges,
  };
}

function parseSourceState(value: JsonValue | null): ReviewGateSourceState | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const kind = asString(value.kind);
  const sourceExecutionProcessId = asString(value.sourceExecutionProcessId);
  const projectId = asString(value.projectId);
  const taskId = asString(value.taskId);
  const workspaceId = asString(value.workspaceId);
  const worktreePath = asString(value.worktreePath);
  const worktreeBranch = asString(value.worktreeBranch);
  const reviewConversationTitle = asString(value.reviewConversationTitle);
  const reviewInitialMessage = asString(value.reviewInitialMessage);
  const repoIds = asStringArray(value.repoIds);
  const reviewConversationId =
    value.reviewConversationId === null
      ? null
      : asString(value.reviewConversationId);
  const reviewExecutionProcessId =
    value.reviewExecutionProcessId === null
      ? null
      : asString(value.reviewExecutionProcessId);
  const registeredAt = value.registeredAt === null ? null : asString(value.registeredAt);
  const outcome = value.outcome === null ? null : parseOutcomeState(value.outcome);

  if (
    kind !== 'review-gate-source' ||
    !sourceExecutionProcessId ||
    !projectId ||
    !taskId ||
    !workspaceId ||
    worktreePath === null ||
    worktreeBranch === null ||
    !reviewConversationTitle ||
    !reviewInitialMessage ||
    repoIds === null ||
    (reviewConversationId !== null && typeof reviewConversationId !== 'string') ||
    (reviewExecutionProcessId !== null && typeof reviewExecutionProcessId !== 'string') ||
    (registeredAt !== null && typeof registeredAt !== 'string') ||
    (value.outcome !== null && outcome === null)
  ) {
    return null;
  }

  return {
    kind,
    sourceExecutionProcessId,
    projectId,
    taskId,
    workspaceId,
    worktreePath,
    worktreeBranch,
    reviewConversationTitle,
    reviewInitialMessage,
    repoIds,
    reviewConversationId,
    reviewExecutionProcessId,
    registeredAt,
    outcome,
  };
}

function parsePointerState(value: JsonValue | null): ReviewGatePointerState | null {
  if (!isJsonRecord(value)) {
    return null;
  }

  const kind = asString(value.kind);
  const sourceExecutionProcessId = asString(value.sourceExecutionProcessId);
  const reviewExecutionProcessId = asString(value.reviewExecutionProcessId);

  if (
    kind !== 'review-gate-review-pointer' ||
    !sourceExecutionProcessId ||
    !reviewExecutionProcessId
  ) {
    return null;
  }

  return {
    kind,
    sourceExecutionProcessId,
    reviewExecutionProcessId,
  };
}

function saveSourceRecord(
  stateStore: OrchestratorStateStore,
  state: ReviewGateSourceState,
): ReviewGateCorrelation {
  const record = stateStore.upsertReviewCorrelation({
    correlationKey: reviewSourceCorrelationKey(state.sourceExecutionProcessId),
    provider: REVIEW_GATE_PROVIDER,
    externalReviewId: state.sourceExecutionProcessId,
    workflowKey: REVIEW_GATE_SOURCE_WORKFLOW_KEY,
    taskId: state.taskId,
    conversationId: state.reviewConversationId,
    reviewAttentionId: state.outcome?.reviewAttentionId ?? null,
    executionProcessId: state.reviewExecutionProcessId,
    state,
  });

  if (state.reviewExecutionProcessId) {
    stateStore.upsertReviewCorrelation({
      correlationKey: reviewExecutionCorrelationKey(state.reviewExecutionProcessId),
      provider: REVIEW_GATE_PROVIDER,
      externalReviewId: state.reviewExecutionProcessId,
      workflowKey: REVIEW_GATE_RESULT_WORKFLOW_KEY,
      taskId: state.taskId,
      conversationId: state.reviewConversationId,
      reviewAttentionId: state.outcome?.reviewAttentionId ?? null,
      executionProcessId: state.sourceExecutionProcessId,
      state: {
        kind: 'review-gate-review-pointer',
        sourceExecutionProcessId: state.sourceExecutionProcessId,
        reviewExecutionProcessId: state.reviewExecutionProcessId,
      } satisfies ReviewGatePointerState,
    });
  }

  return {
    record,
    state,
  };
}

export function reviewSourceCorrelationKey(sourceExecutionProcessId: string): string {
  return `review-gate:source:${sourceExecutionProcessId}`;
}

export function reviewExecutionCorrelationKey(reviewExecutionProcessId: string): string {
  return `review-gate:review:${reviewExecutionProcessId}`;
}

export function reviewConversationClaimKey(sourceExecutionProcessId: string): string {
  return `review-gate:create-conversation:${sourceExecutionProcessId}`;
}

export function reviewAttentionClaimKey(sourceExecutionProcessId: string): string {
  return `review-gate:create-review-attention:${sourceExecutionProcessId}`;
}

export function reviewCommitMessageClaimKey(
  sourceExecutionProcessId: string,
  repoId: string,
): string {
  return `review-gate:generate-commit-message:${sourceExecutionProcessId}:${repoId}`;
}

export function reviewQueueMergeClaimKey(
  sourceExecutionProcessId: string,
  repoId: string,
): string {
  return `review-gate:queue-merge:${sourceExecutionProcessId}:${repoId}`;
}

export function createPendingReviewCorrelation(input: {
  projectId: string;
  taskId: string;
  workspaceId: string;
  sourceExecutionProcessId: string;
  worktreePath: string;
  worktreeBranch: string;
  reviewConversationTitle: string;
  reviewInitialMessage: string;
  repoIds: string[];
}): ReviewGateSourceState {
  return {
    kind: 'review-gate-source',
    sourceExecutionProcessId: input.sourceExecutionProcessId,
    projectId: input.projectId,
    taskId: input.taskId,
    workspaceId: input.workspaceId,
    worktreePath: input.worktreePath,
    worktreeBranch: input.worktreeBranch,
    reviewConversationTitle: input.reviewConversationTitle,
    reviewInitialMessage: input.reviewInitialMessage,
    repoIds: [...input.repoIds],
    reviewConversationId: null,
    reviewExecutionProcessId: null,
    registeredAt: null,
    outcome: null,
  };
}

export function getReviewCorrelationBySource(
  stateStore: OrchestratorStateStore,
  sourceExecutionProcessId: string,
): ReviewGateCorrelation | null {
  const record = stateStore.getReviewCorrelation(
    reviewSourceCorrelationKey(sourceExecutionProcessId),
  );
  if (!record) {
    return null;
  }

  const state = parseSourceState(record.state);
  if (!state) {
    throw new Error(
      `Invalid review source correlation payload for ${sourceExecutionProcessId}`,
    );
  }

  return {
    record,
    state,
  };
}

export function getReviewCorrelationByReviewExecution(
  stateStore: OrchestratorStateStore,
  reviewExecutionProcessId: string,
): ReviewGateCorrelation | null {
  const pointer = stateStore.getReviewCorrelation(
    reviewExecutionCorrelationKey(reviewExecutionProcessId),
  );
  if (!pointer) {
    return null;
  }

  const pointerState = parsePointerState(pointer.state);
  if (!pointerState) {
    throw new Error(
      `Invalid review pointer correlation payload for ${reviewExecutionProcessId}`,
    );
  }

  return getReviewCorrelationBySource(
    stateStore,
    pointerState.sourceExecutionProcessId,
  );
}

export function savePendingReviewCorrelation(
  stateStore: OrchestratorStateStore,
  state: ReviewGateSourceState,
): ReviewGateCorrelation {
  return saveSourceRecord(stateStore, state);
}

export function mutateReviewCorrelation(
  stateStore: OrchestratorStateStore,
  sourceExecutionProcessId: string,
  mutate: ReviewGateMutation,
): ReviewGateCorrelation {
  const existing = getReviewCorrelationBySource(stateStore, sourceExecutionProcessId);
  if (!existing) {
    throw new Error(
      `Missing review correlation for source execution ${sourceExecutionProcessId}`,
    );
  }

  return saveSourceRecord(stateStore, mutate(existing.state));
}
