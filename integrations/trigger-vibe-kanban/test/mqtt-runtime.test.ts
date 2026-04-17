import { afterEach, describe, expect, it } from 'bun:test';
import { connect } from 'mqtt';
import { join } from 'node:path';

import { ORCHESTRATION_SCHEMA_VERSION } from '../../../shared/orchestration-events';

import { startMqttBridgeRuntime } from '../src/runtime/mqtt-bridge';
import { createSqliteStateStore } from '../src/state/sqlite-state-store';
import type { OrchestratorDispatcher } from '../src/runtime/contracts';
import type { RuntimeDependencies } from '../src/runtime/dependencies';
import { createConsoleLogger } from '../src/runtime/dependencies';
import { VkRuntimeClient } from '../src/vk/runtime-client';
import {
  createBroker,
  createJsonServer,
  createTempDir,
  createTestOpenClawSessionConfigResolver,
  createTestTriggerExecutorMapping,
  removeTempDir,
} from './helpers';

const tempDirs = [] as string[];

afterEach(() => {
  while (tempDirs.length > 0) {
    const next = tempDirs.pop();
    if (next) {
      removeTempDir(next);
    }
  }
});

describe('mqtt bridge runtime', () => {
  it('claims replayed MQTT events, hydrates VK contexts, and dispatches stable payloads', async () => {
    const broker = await createBroker();
    const httpServer = await createJsonServer((request, response) => {
      if (
        request.method === 'GET' &&
        request.url === '/api/execution-processes/exec-1/orchestration-context'
      ) {
        response.setHeader('content-type', 'application/json');
        response.end(
          JSON.stringify({
            success: true,
            data: {
              execution: {
                id: 'exec-1',
              },
              scope: {
                scope: 'task',
              },
              coding_agent_turn: null,
              repo_states: [],
              current_execution_visibility: null,
              pending_tool_approvals: [],
              pending_questions: [],
              review_attention: null,
              feedback: null,
            },
          }),
        );
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/approvals/approval-1/orchestration-context'
      ) {
        response.setHeader('content-type', 'application/json');
        response.end(
          JSON.stringify({
            success: true,
            data: {
              approval: {
                id: 'approval-1',
                execution_process_id: 'exec-1',
              },
              execution: null,
              task: null,
              workspace: null,
              session: null,
              current_execution_visibility: null,
            },
          }),
        );
        return;
      }

      response.statusCode = 404;
      response.end('not found');
    });

    const tempDir = createTempDir('trigger-mqtt-');
    tempDirs.push(tempDir);

    const dependencies: RuntimeDependencies = {
      environment: {
        vkApi: {
          baseUrl: `http://127.0.0.1:${httpServer.port}`,
          authMode: 'none',
        },
        vkMqtt: {
          brokerUrl: `mqtt://127.0.0.1:${broker.port}`,
          topicNamespace: 'vk/orchestration',
        },
        stateDatabasePath: join(tempDir, 'state.sqlite'),
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        codeRabbitPollScopeKey: 'global',
        mqttRouterMode: 'direct',
        triggerExecutorMapping: createTestTriggerExecutorMapping(),
        openClawSessionConfigResolver: createTestOpenClawSessionConfigResolver(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: `http://127.0.0.1:${httpServer.port}`,
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
      openClawConversationExecutor: {
        async run() {
          throw new Error('OpenClaw conversation executor should not run in this test');
        },
      },
    };

    const received = [] as Parameters<OrchestratorDispatcher['dispatchOrchestrationEvent']>[0][];
    const dispatcher: OrchestratorDispatcher = {
      async dispatchOrchestrationEvent(input) {
        received.push(input);
        return {
          disposition: 'handled',
          handlerKeys: ['test-handler'],
          output: {
            approvalId: input.contexts.approval?.approval.id ?? null,
          },
        };
      },
      async dispatchScheduledWorkflow() {
        return {
          disposition: 'skipped',
          handlerKeys: [],
          output: null,
          nextCheckpoint: undefined,
        };
      },
    };

    const runtime = await startMqttBridgeRuntime({ dependencies, dispatcher });
    const publisher = connect(dependencies.environment.vkMqtt.brokerUrl);
    await new Promise<void>((resolve, reject) => {
      publisher.once('connect', () => resolve());
      publisher.once('error', (error) => reject(error));
    });

    publisher.publish(
      'vk/orchestration/approval_requested',
      JSON.stringify({
        event_id: 'evt-1',
        schema_version: ORCHESTRATION_SCHEMA_VERSION,
        occurred_at: '2026-04-15T10:00:00Z',
        event_type: 'approval_requested',
        task_id: null,
        workspace_id: null,
        session_id: null,
        execution_process_id: 'exec-1',
        task_group_id: null,
        payload: {
          approval_id: 'approval-1',
          kind: 'tool_approval',
          tool_call_id: 'tool-call-1',
          tool_name: 'agent-browser',
          question_count: 0,
        },
      }),
    );

    await Bun.sleep(150);

    publisher.publish(
      'vk/orchestration/approval_requested',
      JSON.stringify({
        event_id: 'evt-1',
        schema_version: ORCHESTRATION_SCHEMA_VERSION,
        occurred_at: '2026-04-15T10:00:00Z',
        event_type: 'approval_requested',
        task_id: null,
        workspace_id: null,
        session_id: null,
        execution_process_id: 'exec-1',
        task_group_id: null,
        payload: {
          approval_id: 'approval-1',
          kind: 'tool_approval',
          tool_call_id: 'tool-call-1',
          tool_name: 'agent-browser',
          question_count: 0,
        },
      }),
    );

    await Bun.sleep(150);

    expect(received).toHaveLength(1);
    expect(received[0]?.event.eventId).toBe('evt-1');
    expect(received[0]?.contexts.approval?.approval.id).toBe('approval-1');
    expect(dependencies.stateStore.getClaim('evt-1')?.status).toBe('completed');

    await runtime.close();
    dependencies.stateStore.close();
    publisher.end(true);
    await broker.close();
    await httpServer.close();
  });
});
