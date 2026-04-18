import { mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { Database } from 'bun:sqlite';

import type {
  ClaimInput,
  ClaimRecord,
  ClaimResult,
  JsonValue,
  OpenClawConversationSessionRecord,
  OrchestratorStateStore,
  ReviewCorrelationRecord,
  WorkflowCheckpointRecord,
} from './types';

type ClaimRow = {
  claim_key: string;
  claim_kind: 'event' | 'schedule';
  workflow_key: string;
  scope_key: string;
  status: 'claimed' | 'dispatched' | 'completed' | 'failed';
  claimed_at: string;
  updated_at: string;
  metadata_json: string | null;
  result_json: string | null;
  error_text: string | null;
};

type CheckpointRow = {
  workflow_key: string;
  scope_key: string;
  checkpoint_json: string | null;
  last_claim_key: string;
  updated_at: string;
};

type ReviewCorrelationRow = {
  correlation_key: string;
  provider: string;
  external_review_id: string;
  workflow_key: string;
  task_id: string | null;
  conversation_id: string | null;
  review_attention_id: string | null;
  execution_process_id: string | null;
  state_json: string | null;
  created_at: string;
  updated_at: string;
};

type OpenClawConversationSessionRow = {
  correlation_key: string;
  idempotency_key: string;
  workflow_key: string;
  scope_key: string;
  session_key: string;
  session_id: string | null;
  profile_name: string;
  engine_model: string;
  working_directory: string;
  status: 'active' | 'cleaned';
  created_at: string;
  updated_at: string;
};

const OPENCLAW_CONVERSATION_SESSION_TABLE =
  'openclaw_conversation_sessions_v2';

function serializeJson(value: JsonValue | null | undefined): string | null {
  if (value === undefined || value === null) {
    return null;
  }

  return JSON.stringify(value);
}

function parseJson(value: string | null): JsonValue | null {
  if (value === null) {
    return null;
  }

  return JSON.parse(value) as JsonValue;
}

function mapClaim(row: ClaimRow): ClaimRecord {
  return {
    claimKey: row.claim_key,
    claimKind: row.claim_kind,
    workflowKey: row.workflow_key,
    scopeKey: row.scope_key,
    status: row.status,
    claimedAt: row.claimed_at,
    updatedAt: row.updated_at,
    metadata: parseJson(row.metadata_json),
    result: parseJson(row.result_json),
    error: row.error_text,
  };
}

function mapCheckpoint(row: CheckpointRow): WorkflowCheckpointRecord {
  return {
    workflowKey: row.workflow_key,
    scopeKey: row.scope_key,
    checkpoint: parseJson(row.checkpoint_json),
    lastClaimKey: row.last_claim_key,
    updatedAt: row.updated_at,
  };
}

function mapReviewCorrelation(row: ReviewCorrelationRow): ReviewCorrelationRecord {
  return {
    correlationKey: row.correlation_key,
    provider: row.provider,
    externalReviewId: row.external_review_id,
    workflowKey: row.workflow_key,
    taskId: row.task_id,
    conversationId: row.conversation_id,
    reviewAttentionId: row.review_attention_id,
    executionProcessId: row.execution_process_id,
    state: parseJson(row.state_json),
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  };
}

function mapOpenClawConversationSession(
  row: OpenClawConversationSessionRow,
): OpenClawConversationSessionRecord {
  return {
    correlationKey: row.correlation_key,
    idempotencyKey: row.idempotency_key,
    workflowKey: row.workflow_key,
    scopeKey: row.scope_key,
    sessionKey: row.session_key,
    sessionId: row.session_id,
    profileName: row.profile_name,
    engineModel: row.engine_model,
    workingDirectory: row.working_directory,
    status: row.status,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  };
}

export function createSqliteStateStore(databasePath: string): OrchestratorStateStore {
  mkdirSync(dirname(databasePath), { recursive: true });

  const database = new Database(databasePath, { create: true, strict: true });

  database.exec('PRAGMA journal_mode = WAL;');
  database.exec('PRAGMA foreign_keys = ON;');

  database.exec(`
    CREATE TABLE IF NOT EXISTS orchestrator_claims (
      claim_key TEXT PRIMARY KEY,
      claim_kind TEXT NOT NULL,
      workflow_key TEXT NOT NULL,
      scope_key TEXT NOT NULL,
      status TEXT NOT NULL,
      claimed_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      metadata_json TEXT,
      result_json TEXT,
      error_text TEXT
    );

    CREATE INDEX IF NOT EXISTS idx_orchestrator_claims_workflow_scope
      ON orchestrator_claims(workflow_key, scope_key);

    CREATE TABLE IF NOT EXISTS workflow_checkpoints (
      workflow_key TEXT NOT NULL,
      scope_key TEXT NOT NULL,
      checkpoint_json TEXT,
      last_claim_key TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      PRIMARY KEY (workflow_key, scope_key)
    );

    CREATE TABLE IF NOT EXISTS review_correlations (
      correlation_key TEXT PRIMARY KEY,
      provider TEXT NOT NULL,
      external_review_id TEXT NOT NULL,
      workflow_key TEXT NOT NULL,
      task_id TEXT,
      conversation_id TEXT,
      review_attention_id TEXT,
      execution_process_id TEXT,
      state_json TEXT,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS ${OPENCLAW_CONVERSATION_SESSION_TABLE} (
      correlation_key TEXT NOT NULL,
      idempotency_key TEXT NOT NULL,
      workflow_key TEXT NOT NULL,
      scope_key TEXT NOT NULL,
      session_key TEXT NOT NULL,
      session_id TEXT,
      profile_name TEXT NOT NULL,
      engine_model TEXT NOT NULL,
      working_directory TEXT NOT NULL,
      status TEXT NOT NULL,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      PRIMARY KEY (correlation_key, idempotency_key)
    );
  `);

  const getClaimStatement = database.query<ClaimRow, [string]>(`
    SELECT
      claim_key,
      claim_kind,
      workflow_key,
      scope_key,
      status,
      claimed_at,
      updated_at,
      metadata_json,
      result_json,
      error_text
    FROM orchestrator_claims
    WHERE claim_key = ?1
  `);

  const insertClaimStatement = database.query(
    `
      INSERT INTO orchestrator_claims (
        claim_key,
        claim_kind,
        workflow_key,
        scope_key,
        status,
        claimed_at,
        updated_at,
        metadata_json,
        result_json,
        error_text
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, NULL, NULL)
    `,
  );

  const updateClaimStatement = database.query(
    `
      UPDATE orchestrator_claims
      SET
        status = ?2,
        updated_at = ?3,
        result_json = ?4,
        error_text = ?5
      WHERE claim_key = ?1
    `,
  );

  const getCheckpointStatement = database.query<CheckpointRow, [string, string]>(`
    SELECT workflow_key, scope_key, checkpoint_json, last_claim_key, updated_at
    FROM workflow_checkpoints
    WHERE workflow_key = ?1 AND scope_key = ?2
  `);

  const upsertCheckpointStatement = database.query(
    `
      INSERT INTO workflow_checkpoints (
        workflow_key,
        scope_key,
        checkpoint_json,
        last_claim_key,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5)
      ON CONFLICT(workflow_key, scope_key)
      DO UPDATE SET
        checkpoint_json = excluded.checkpoint_json,
        last_claim_key = excluded.last_claim_key,
        updated_at = excluded.updated_at
    `,
  );

  const getReviewCorrelationStatement = database.query<ReviewCorrelationRow, [string]>(`
    SELECT
      correlation_key,
      provider,
      external_review_id,
      workflow_key,
      task_id,
      conversation_id,
      review_attention_id,
      execution_process_id,
      state_json,
      created_at,
      updated_at
    FROM review_correlations
    WHERE correlation_key = ?1
  `);

  const upsertReviewCorrelationStatement = database.query(
    `
      INSERT INTO review_correlations (
        correlation_key,
        provider,
        external_review_id,
        workflow_key,
        task_id,
        conversation_id,
        review_attention_id,
        execution_process_id,
        state_json,
        created_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
      ON CONFLICT(correlation_key)
      DO UPDATE SET
        provider = excluded.provider,
        external_review_id = excluded.external_review_id,
        workflow_key = excluded.workflow_key,
        task_id = excluded.task_id,
        conversation_id = excluded.conversation_id,
        review_attention_id = excluded.review_attention_id,
        execution_process_id = excluded.execution_process_id,
        state_json = excluded.state_json,
        updated_at = excluded.updated_at
    `,
  );

  const getOpenClawConversationSessionStatement = database.query<
    OpenClawConversationSessionRow,
    [string, string]
  >(`
    SELECT
      correlation_key,
      idempotency_key,
      workflow_key,
      scope_key,
      session_key,
      session_id,
      profile_name,
      engine_model,
      working_directory,
      status,
      created_at,
      updated_at
    FROM ${OPENCLAW_CONVERSATION_SESSION_TABLE}
    WHERE correlation_key = ?1 AND idempotency_key = ?2
  `);

  const upsertOpenClawConversationSessionStatement = database.query(
    `
      INSERT INTO ${OPENCLAW_CONVERSATION_SESSION_TABLE} (
        correlation_key,
        idempotency_key,
        workflow_key,
        scope_key,
        session_key,
        session_id,
        profile_name,
        engine_model,
        working_directory,
        status,
        created_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
      ON CONFLICT(correlation_key, idempotency_key)
      DO UPDATE SET
        workflow_key = excluded.workflow_key,
        scope_key = excluded.scope_key,
        session_key = excluded.session_key,
        session_id = excluded.session_id,
        profile_name = excluded.profile_name,
        engine_model = excluded.engine_model,
        working_directory = excluded.working_directory,
        status = excluded.status,
        updated_at = excluded.updated_at
    `,
  );

  function requireClaim(claimKey: string): ClaimRecord {
    const row = getClaimStatement.get(claimKey);
    if (!row) {
      throw new Error(`Unknown claim '${claimKey}'`);
    }

    return mapClaim(row);
  }

  function claim(input: ClaimInput): ClaimResult {
    const now = new Date().toISOString();
    const existing = getClaimStatement.get(input.claimKey);
    if (existing) {
      return {
        claimed: false,
        record: mapClaim(existing),
      };
    }

    insertClaimStatement.run(
      input.claimKey,
      input.claimKind,
      input.workflowKey,
      input.scopeKey,
      'claimed',
      now,
      serializeJson(input.metadata ?? null),
    );

    return {
      claimed: true,
      record: requireClaim(input.claimKey),
    };
  }

  function updateClaim(
    claimKey: string,
    status: ClaimRecord['status'],
    result: JsonValue | null,
    error: string | null,
  ): ClaimRecord {
    const now = new Date().toISOString();
    updateClaimStatement.run(
      claimKey,
      status,
      now,
      serializeJson(result),
      error,
    );

    return requireClaim(claimKey);
  }

  return {
    claim,
    claimEventId(input) {
      return claim({ ...input, claimKind: 'event' });
    },
    claimScheduledRun(input) {
      return claim({ ...input, claimKind: 'schedule' });
    },
    markClaimDispatched(claimKey, result = null) {
      return updateClaim(claimKey, 'dispatched', result, null);
    },
    markClaimCompleted(claimKey, result = null) {
      return updateClaim(claimKey, 'completed', result, null);
    },
    markClaimFailed(claimKey, error) {
      return updateClaim(claimKey, 'failed', null, error);
    },
    getClaim(claimKey) {
      const row = getClaimStatement.get(claimKey);
      return row ? mapClaim(row) : null;
    },
    getCheckpoint(workflowKey, scopeKey) {
      const row = getCheckpointStatement.get(workflowKey, scopeKey);
      return row ? mapCheckpoint(row) : null;
    },
    putCheckpoint(input) {
      const now = new Date().toISOString();
      upsertCheckpointStatement.run(
        input.workflowKey,
        input.scopeKey,
        serializeJson(input.checkpoint),
        input.claimKey,
        now,
      );

      const row = getCheckpointStatement.get(input.workflowKey, input.scopeKey);
      if (!row) {
        throw new Error(
          `Missing checkpoint '${input.workflowKey}:${input.scopeKey}' after upsert`,
        );
      }

      return mapCheckpoint(row);
    },
    getReviewCorrelation(correlationKey) {
      const row = getReviewCorrelationStatement.get(correlationKey);
      return row ? mapReviewCorrelation(row) : null;
    },
    upsertReviewCorrelation(input) {
      const now = new Date().toISOString();
      upsertReviewCorrelationStatement.run(
        input.correlationKey,
        input.provider,
        input.externalReviewId,
        input.workflowKey,
        input.taskId,
        input.conversationId,
        input.reviewAttentionId,
        input.executionProcessId,
        serializeJson(input.state),
        now,
      );

      const row = getReviewCorrelationStatement.get(input.correlationKey);
      if (!row) {
        throw new Error(
          `Missing review correlation '${input.correlationKey}' after upsert`,
        );
      }

      return mapReviewCorrelation(row);
    },
    getOpenClawConversationSession(correlationKey, idempotencyKey) {
      const row = getOpenClawConversationSessionStatement.get(
        correlationKey,
        idempotencyKey,
      );
      return row ? mapOpenClawConversationSession(row) : null;
    },
    upsertOpenClawConversationSession(input) {
      const now = new Date().toISOString();
      upsertOpenClawConversationSessionStatement.run(
        input.correlationKey,
        input.idempotencyKey,
        input.workflowKey,
        input.scopeKey,
        input.sessionKey,
        input.sessionId,
        input.profileName,
        input.engineModel,
        input.workingDirectory,
        input.status,
        now,
      );

      const row = getOpenClawConversationSessionStatement.get(
        input.correlationKey,
        input.idempotencyKey,
      );
      if (!row) {
        throw new Error(
          `Missing OpenClaw conversation session '${input.correlationKey}/${input.idempotencyKey}' after upsert`,
        );
      }

      return mapOpenClawConversationSession(row);
    },
    close() {
      database.close();
    },
  } satisfies OrchestratorStateStore;
}
