export type JsonPrimitive = string | number | boolean | null;

export type JsonValue =
  | JsonPrimitive
  | { [key: string]: JsonValue }
  | JsonValue[];

export type ClaimKind = 'event' | 'schedule';

export type ClaimStatus = 'claimed' | 'dispatched' | 'completed' | 'failed';

export type ClaimRecord = {
  claimKey: string;
  claimKind: ClaimKind;
  workflowKey: string;
  scopeKey: string;
  status: ClaimStatus;
  claimedAt: string;
  updatedAt: string;
  metadata: JsonValue | null;
  result: JsonValue | null;
  error: string | null;
};

export type ClaimResult = {
  claimed: boolean;
  record: ClaimRecord;
};

export type WorkflowCheckpointRecord = {
  workflowKey: string;
  scopeKey: string;
  checkpoint: JsonValue | null;
  lastClaimKey: string;
  updatedAt: string;
};

export type ReviewCorrelationRecord = {
  correlationKey: string;
  provider: string;
  externalReviewId: string;
  workflowKey: string;
  taskId: string | null;
  conversationId: string | null;
  reviewAttentionId: string | null;
  executionProcessId: string | null;
  state: JsonValue | null;
  createdAt: string;
  updatedAt: string;
};

export type ClaimInput = {
  claimKey: string;
  claimKind: ClaimKind;
  workflowKey: string;
  scopeKey: string;
  metadata?: JsonValue | null;
};

export interface OrchestratorStateStore {
  claim(input: ClaimInput): ClaimResult;
  claimEventId(input: Omit<ClaimInput, 'claimKind'>): ClaimResult;
  claimScheduledRun(input: Omit<ClaimInput, 'claimKind'>): ClaimResult;
  markClaimDispatched(claimKey: string, result?: JsonValue | null): ClaimRecord;
  markClaimCompleted(claimKey: string, result?: JsonValue | null): ClaimRecord;
  markClaimFailed(claimKey: string, error: string): ClaimRecord;
  getClaim(claimKey: string): ClaimRecord | null;
  getCheckpoint(
    workflowKey: string,
    scopeKey: string,
  ): WorkflowCheckpointRecord | null;
  putCheckpoint(input: {
    workflowKey: string;
    scopeKey: string;
    claimKey: string;
    checkpoint: JsonValue | null;
  }): WorkflowCheckpointRecord;
  getReviewCorrelation(correlationKey: string): ReviewCorrelationRecord | null;
  upsertReviewCorrelation(input: Omit<ReviewCorrelationRecord, 'createdAt' | 'updatedAt'>): ReviewCorrelationRecord;
  close(): void;
}
