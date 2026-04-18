import { describe, expect, it } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
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
  createTestOpenClawSessionConfigResolver,
  createTestTriggerExecutorMapping,
  removeTempDir,
} from './helpers';

describe('package boot surfaces', () => {
  it('audits the Trigger integration tree for legacy plugin-boundary usage while allowing the standalone SessionManager package seam', () => {
    const integrationRoot = join(import.meta.dir, '..');
    const disallowedMatches: string[] = [];
    let scannedFiles = 0;

    function walk(directory: string) {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        if (
          entry.name === 'node_modules' ||
          entry.name === '.git' ||
          entry.name === 'dist'
        ) {
          continue;
        }

        const absolutePath = join(directory, entry.name);
        if (entry.isDirectory()) {
          walk(absolutePath);
          continue;
        }

        if (absolutePath.endsWith(join('test', 'boot.test.ts'))) {
          continue;
        }

        scannedFiles += 1;
        const contents = readFileSync(absolutePath, 'utf8');

        if (/openclaw\/plugin-sdk/.test(contents)) {
          disallowedMatches.push(`${absolutePath}:openclaw/plugin-sdk`);
        }

        // The package root is the plugin entrypoint. Trigger should consume the
        // standalone SessionManager seam via explicit subpath imports instead.
        if (
          /from ['"]@enderfga\/openclaw-claude-code['"]/.test(contents) ||
          /from ['"]@enderfga\/openclaw-claude-code['"];?/.test(contents)
        ) {
          disallowedMatches.push(`${absolutePath}:plugin package root import`);
        }

        if (/claude-code-skill serve/.test(contents)) {
          disallowedMatches.push(`${absolutePath}:claude-code-skill serve`);
        }

        if (
          absolutePath.endsWith('package.json') &&
          /"openclaw"\s*:/.test(contents)
        ) {
          disallowedMatches.push(`${absolutePath}:direct package dependency openclaw`);
        }
      }
    }

    walk(integrationRoot);

    // Bun lockfiles can still include transitive peer metadata for the allowed
    // standalone package. We only reject direct workspace ownership of the
    // `openclaw` package itself.
    const bunLockContents = readFileSync(join(integrationRoot, 'bun.lock'), 'utf8');
    const workspaceDependencies =
      bunLockContents.match(
        /"dependencies":\s*\{([\s\S]*?)\n\s*\},\n\s*"devDependencies"/,
      )?.[1] ?? '';

    if (/"openclaw"\s*:/.test(workspaceDependencies)) {
      disallowedMatches.push('bun.lock:direct workspace dependency openclaw');
    }

    expect(scannedFiles).toBeGreaterThan(0);
    expect(disallowedMatches).toEqual([]);
  });

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
        openClawSessionConfigResolver: createTestOpenClawSessionConfigResolver(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: `http://127.0.0.1:${vkServer.port}`,
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
      openClawConversationExecutor: {
        async run() {
          throw new Error('OpenClaw conversation executor should not run in this test');
        },
      },
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
