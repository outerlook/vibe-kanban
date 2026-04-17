import { describe, expect, it } from 'bun:test';
import { join } from 'node:path';

import { ORCHESTRATION_SCHEMA_VERSION } from '../../../shared/orchestration-events';

import { createSqliteStateStore } from '../src/state/sqlite-state-store';
import { createConsoleLogger } from '../src/runtime/dependencies';
import { createTriggerDispatcher } from '../src/trigger/trigger-dispatcher';
import { VkRuntimeClient } from '../src/vk/runtime-client';
import {
  createTempDir,
  createTestOpenClawSessionConfigResolver,
  createTestTriggerExecutorMapping,
  removeTempDir,
} from './helpers';

describe('trigger dispatcher', () => {
  it('routes MQTT events into Trigger task runs by default', async () => {
    const previousSecret = process.env.TRIGGER_SECRET_KEY;
    process.env.TRIGGER_SECRET_KEY = 'tr_dev_test_key';

    const tempDir = createTempDir('trigger-dispatcher-');

    const dependencies = {
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
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        codeRabbitPollScopeKey: 'global',
        mqttRouterMode: 'trigger' as const,
        triggerExecutorMapping: createTestTriggerExecutorMapping(),
        openClawSessionConfigResolver: createTestOpenClawSessionConfigResolver(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: 'http://127.0.0.1:9',
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
      openClawConversationExecutor: {
        async run() {
          throw new Error('OpenClaw conversation executor should not run in this test');
        },
      },
    };

    const calls: unknown[] = [];
    const dispatcher = createTriggerDispatcher(
      dependencies,
      (async (...args: unknown[]) => {
        calls.push(args);
        return { id: 'run_123' };
      }) as never,
    );

    const result = await dispatcher.dispatchOrchestrationEvent({
      claim: {
        claimKey: 'evt-1',
        claimKind: 'event',
        workflowKey: 'mqtt/approval_requested',
        scopeKey: 'vk/orchestration/approval_requested',
        status: 'claimed',
        claimedAt: '2026-04-15T10:00:00Z',
        updatedAt: '2026-04-15T10:00:00Z',
        metadata: null,
        result: null,
        error: null,
      },
      event: {
        topic: 'vk/orchestration/approval_requested',
        eventId: 'evt-1',
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        occurredAt: '2026-04-15T10:00:00Z',
        eventType: 'approval_requested',
        entityIds: {
          taskId: null,
          workspaceId: null,
          sessionId: null,
          executionProcessId: 'exec-1',
          taskGroupId: null,
        },
        payload: {
          approval_id: 'approval-1',
          kind: 'tool_approval',
          tool_call_id: 'tool-call-1',
          tool_name: 'agent-browser',
          question_count: 0,
        },
        refs: {
          taskContext: null,
          taskGroupContext: null,
          conversationContext: null,
          executionContext: { executionProcessId: 'exec-1' },
          approvalContext: { approvalId: 'approval-1' },
          taskSession: null,
        },
      },
      contexts: {
        task: null,
        taskGroup: null,
        conversation: null,
        execution: null,
        approval: null,
      },
    });

    expect(calls).toHaveLength(1);
    expect(result.output).toEqual({ route: 'trigger', runId: 'run_123' });

    dependencies.stateStore.close();
    removeTempDir(tempDir);
    if (previousSecret === undefined) {
      delete process.env.TRIGGER_SECRET_KEY;
    } else {
      process.env.TRIGGER_SECRET_KEY = previousSecret;
    }
  });
});
