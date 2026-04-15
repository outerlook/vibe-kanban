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
import { createBroker, createTempDir, removeTempDir } from './helpers';

describe('package boot surfaces', () => {
  it('boots the scheduled scaffold without needing VK-owned checkpoint state', async () => {
    const tempDir = createTempDir('trigger-boot-');

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
        mqttRouterMode: 'direct' as const,
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: 'http://127.0.0.1:9',
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
    };

    const result = await runScheduledWorkflowOnce(
      {
        workflowKey: 'coderabbit/poll',
        scopeKey: 'global',
        claimKey: 'coderabbit:boot-test',
        scheduledAt: '2026-04-15T12:00:00Z',
      },
      dependencies,
      createDirectDispatcher(dependencies),
    );

    expect(result.disposition).toBe('handled');
    expect(result.handlerKeys).toEqual(['coderabbit/poll']);
    expect(dependencies.stateStore.getCheckpoint('coderabbit/poll', 'global')?.checkpoint).toBeNull();

    dependencies.stateStore.close();
    removeTempDir(tempDir);
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
