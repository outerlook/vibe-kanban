import { afterEach, describe, expect, it } from 'bun:test';
import { join } from 'node:path';

import { createSqliteStateStore } from '../src/state/sqlite-state-store';
import { createTempDir, removeTempDir } from './helpers';

const tempDirs = [] as string[];

afterEach(() => {
  while (tempDirs.length > 0) {
    const next = tempDirs.pop();
    if (next) {
      removeTempDir(next);
    }
  }
});

describe('sqlite orchestrator state store', () => {
  it('deduplicates replayed MQTT event claims and keeps claim updates idempotent', () => {
    const tempDir = createTempDir('trigger-state-');
    tempDirs.push(tempDir);

    const store = createSqliteStateStore(join(tempDir, 'state.sqlite'));

    const firstClaim = store.claimEventId({
      claimKey: 'evt-1',
      workflowKey: 'mqtt/approval_requested',
      scopeKey: 'vk/orchestration/approval_requested',
      metadata: { eventType: 'approval_requested' },
    });
    const secondClaim = store.claimEventId({
      claimKey: 'evt-1',
      workflowKey: 'mqtt/approval_requested',
      scopeKey: 'vk/orchestration/approval_requested',
      metadata: { eventType: 'approval_requested' },
    });

    expect(firstClaim.claimed).toBe(true);
    expect(secondClaim.claimed).toBe(false);

    store.markClaimDispatched('evt-1', { stage: 'dispatch' });
    const completed = store.markClaimCompleted('evt-1', { stage: 'done' });

    expect(completed.status).toBe('completed');
    expect(completed.result).toEqual({ stage: 'done' });

    store.close();
  });

  it('tracks scheduled checkpoints and review correlations outside VK', () => {
    const tempDir = createTempDir('trigger-state-');
    tempDirs.push(tempDir);

    const store = createSqliteStateStore(join(tempDir, 'state.sqlite'));

    const claim = store.claimScheduledRun({
      claimKey: 'coderabbit:2026-04-15T12:00:00Z',
      workflowKey: 'coderabbit/poll',
      scopeKey: 'global',
      metadata: { scheduledAt: '2026-04-15T12:00:00Z' },
    });

    const checkpoint = store.putCheckpoint({
      workflowKey: 'coderabbit/poll',
      scopeKey: 'global',
      claimKey: claim.record.claimKey,
      checkpoint: {
        cursor: 'review-42',
      },
    });
    const correlation = store.upsertReviewCorrelation({
      correlationKey: 'coderabbit:review-42',
      provider: 'coderabbit',
      externalReviewId: 'review-42',
      workflowKey: 'coderabbit/poll',
      taskId: 'task-1',
      conversationId: 'conversation-1',
      reviewAttentionId: 'attention-1',
      executionProcessId: 'exec-1',
      state: { status: 'seen' },
    });

    expect(checkpoint.checkpoint).toEqual({ cursor: 'review-42' });
    expect(store.getCheckpoint('coderabbit/poll', 'global')?.lastClaimKey).toBe(
      claim.record.claimKey,
    );
    expect(correlation.externalReviewId).toBe('review-42');
    expect(store.getReviewCorrelation('coderabbit:review-42')?.taskId).toBe('task-1');

    store.close();
  });
});
