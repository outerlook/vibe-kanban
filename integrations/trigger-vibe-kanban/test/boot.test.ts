import { describe, expect, it } from 'bun:test';
import { join } from 'node:path';

import { ORCHESTRATION_SCHEMA_VERSION } from '../../../shared/orchestration-events';

import {
  createDirectDispatcher,
  createSqliteStateStore,
  runScheduledWorkflowOnce,
  startMqttBridgeRuntime,
  VkRuntimeClient,
} from '../src';
import { createConsoleLogger } from '../src/runtime/dependencies';
import {
  createBroker,
  createJsonServer,
  createTempDir,
  createTestTriggerExecutorMapping,
  removeTempDir,
} from './helpers';

describe('package boot surfaces', () => {
  it('boots the scheduled CodeRabbit poller with Trigger-owned checkpoint state', async () => {
    const previousGitHubToken = process.env.GITHUB_TOKEN;
    process.env.GITHUB_TOKEN = 'test-token';

    const vkServer = await createJsonServer((request, response) => {
      if (request.method === 'GET' && request.url === '/api/projects') {
        response.setHeader('content-type', 'application/json');
        response.end(JSON.stringify({ success: true, data: [] }));
        return;
      }

      response.statusCode = 404;
      response.end('not found');
    });

    const tempDir = createTempDir('trigger-boot-');

    const dependencies = {
      environment: {
        vkApi: {
          baseUrl: `http://127.0.0.1:${vkServer.port}`,
          authMode: 'none' as const,
        },
        vkMqtt: {
          brokerUrl: 'mqtt://127.0.0.1:1883',
          topicNamespace: 'vk/orchestration',
        },
        stateDatabasePath: join(tempDir, 'state.sqlite'),
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        codeRabbitPollScopeKey: 'global',
        mqttRouterMode: 'direct' as const,
        triggerExecutorMapping: createTestTriggerExecutorMapping(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: `http://127.0.0.1:${vkServer.port}`,
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
    };

    try {
      const result = await runScheduledWorkflowOnce(
        {
          workflowKey: 'coderabbit/poll',
          scopeKey: 'global',
          claimKey: 'coderabbit:boot-test',
          scheduledAt: '2026-04-15T12:00:00Z',
          metadata: {
            workflowId: 'wf-coderabbit-review-extraction',
            reviewerLogins: ['coderabbitai[bot]'],
          },
        },
        dependencies,
        createDirectDispatcher(dependencies),
      );

      expect(result.disposition).toBe('handled');
      expect(result.handlerKeys).toEqual(['coderabbit/poll']);
      expect(dependencies.stateStore.getCheckpoint('coderabbit/poll', 'global')?.checkpoint).toEqual({
        workflowId: 'wf-coderabbit-review-extraction',
        lastPolledAt: '2026-04-15T12:00:00Z',
        selectedReposByFullName: {},
        processedThreadCommentIds: [],
      });
    } finally {
      dependencies.stateStore.close();
      removeTempDir(tempDir);
      await vkServer.close();
      if (previousGitHubToken === undefined) {
        delete process.env.GITHUB_TOKEN;
      } else {
        process.env.GITHUB_TOKEN = previousGitHubToken;
      }
    }
  });

  it('starts the mqtt boot surface and exposes a close handle', async () => {
    const broker = await createBroker();
    const tempDir = createTempDir('trigger-boot-');

    const dependencies = {
      environment: {
        vkApi: {
          baseUrl: 'http://127.0.0.1:9',
          authMode: 'none' as const,
        },
        vkMqtt: {
          brokerUrl: `mqtt://127.0.0.1:${broker.port}`,
          topicNamespace: 'vk/orchestration',
        },
        stateDatabasePath: join(tempDir, 'state.sqlite'),
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        codeRabbitPollScopeKey: 'global',
        mqttRouterMode: 'direct' as const,
        triggerExecutorMapping: createTestTriggerExecutorMapping(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: 'http://127.0.0.1:9',
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
    };

    const runtime = await startMqttBridgeRuntime({
      dependencies,
      eventTypes: ['approval_requested'],
      dispatcher: createDirectDispatcher(dependencies),
    });

    expect(typeof runtime.close).toBe('function');

    await runtime.close();
    dependencies.stateStore.close();
    await broker.close();
    removeTempDir(tempDir);
  });
});
