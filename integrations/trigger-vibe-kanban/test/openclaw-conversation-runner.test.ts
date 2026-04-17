import { afterEach, describe, expect, it } from 'bun:test';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

import {
  cleanupOpenClawConversationRequest,
  createSqliteStateStore,
  createTriggerOpenClawConversationRunner,
  OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
  OPENCLAW_CONVERSATION_TASK_ID,
  runOpenClawConversationRequest,
  type OpenClawConversationRunRequest,
  type OpenClawSdk,
  VkRuntimeClient,
} from '../src';
import { createConsoleLogger } from '../src/runtime/dependencies';
import type { RuntimeDependencies } from '../src/runtime/dependencies';
import { TRIGGER_EXECUTOR_OPERATION_KEYS } from '../src/runtime/executor-mapping';
import {
  createTempDir,
  createTestOpenClawSessionConfigResolver,
  createTestTriggerExecutorMapping,
  removeTempDir,
} from './helpers';

const tempDirs: string[] = [];

afterEach(() => {
  while (tempDirs.length > 0) {
    const next = tempDirs.pop();
    if (next) {
      removeTempDir(next);
    }
  }
});

function createDependencies(prefix: string): RuntimeDependencies {
  const tempDir = createTempDir(prefix);
  tempDirs.push(tempDir);

  return {
    environment: {
      vkApi: {
        baseUrl: 'http://127.0.0.1:9',
        authMode: 'none',
      },
      vkMqtt: {
        brokerUrl: 'mqtt://127.0.0.1:1883',
        topicNamespace: 'vk/orchestration',
      },
      stateDatabasePath: join(tempDir, 'state.sqlite'),
      schemaVersion: 'vk_orchestration_v1',
      codeRabbitPollScopeKey: 'global',
      mqttRouterMode: 'trigger',
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
}

function buildRunRequest(
  overrides: Partial<OpenClawConversationRunRequest> = {},
): OpenClawConversationRunRequest {
  return {
    action: 'run',
    prompt: 'Review this change.',
    response: { kind: 'text' },
    workingDirectory: '/tmp/worktree',
    selection: {
      profileName: 'CLAUDE_CODE.REVIEW',
    },
    correlation: {
      workflowKey: 'review-gate',
      scopeKey: 'task-123',
      correlationKey: 'corr-123',
      idempotencyKey: 'idem-123',
    },
    timeoutMs: 5_000,
    cleanup: {
      onSuccess: 'preserve',
      onError: 'preserve',
    },
    ...overrides,
  };
}

type FakeOpenClawHarness = {
  calls: {
    getReply: number;
    lastConfig: Record<string, unknown> | null;
    lastContext: Record<string, unknown> | null;
  };
  sdk: OpenClawSdk;
  storePath: string;
};

function createFakeOpenClawSdk(options: {
  tempDir: string;
  response?: string;
  onGetReply?: (args: {
    ctx: Record<string, unknown>;
    opts: Record<string, unknown>;
    configOverride: Record<string, unknown>;
    storePath: string;
    sessionKey: string;
    transcriptPath: string;
    stores: Map<string, Record<string, any>>;
  }) => Promise<void> | void;
}): FakeOpenClawHarness {
  const stores = new Map<string, Record<string, any>>();
  const calls = {
    getReply: 0,
    lastConfig: null as Record<string, unknown> | null,
    lastContext: null as Record<string, unknown> | null,
  };
  const storePath = join(options.tempDir, 'openclaw-sessions.json');

  const sdk: OpenClawSdk = {
    applyModelOverrideToSessionEntry({ entry, selection, profileOverride }) {
      entry.modelProvider = selection.provider;
      entry.model = selection.model;
      entry.modelOverride = `${selection.provider}/${selection.model}`;
      if (profileOverride) {
        entry.authProfileOverride = profileOverride;
      }
      return { updated: true };
    },
    async getReplyFromConfig(ctx, opts, configOverride) {
      calls.getReply += 1;
      calls.lastConfig = configOverride as Record<string, unknown>;
      calls.lastContext = ctx as Record<string, unknown>;

      const sessionKey = String(ctx.SessionKey);
      const transcriptPath = join(options.tempDir, `${sessionKey}.jsonl`);
      writeFileSync(transcriptPath, 'assistant output');

      stores.set(storePath, {
        ...(stores.get(storePath) ?? {}),
        [sessionKey]: {
          sessionId: `session-${calls.getReply}`,
          sessionFile: transcriptPath,
        },
      });

      opts?.onAgentRunStart?.(`run-${calls.getReply}`);
      opts?.onModelSelected?.({
        provider: 'anthropic',
        model: 'claude-sonnet-4-5',
        thinkLevel: 'medium',
      } as never);

      await options.onGetReply?.({
        ctx: ctx as Record<string, unknown>,
        opts: opts as Record<string, unknown>,
        configOverride: configOverride as Record<string, unknown>,
        storePath,
        sessionKey,
        transcriptPath,
        stores,
      });

      return { text: options.response ?? 'Looks good to me.' };
    },
    loadSessionStore(path) {
      return structuredClone(stores.get(path) ?? {});
    },
    resolveSessionStoreEntry({ store, sessionKey }) {
      return {
        normalizedKey: sessionKey,
        existing: store[sessionKey],
        legacyKeys: [],
      };
    },
    resolveStorePath(store) {
      return store ?? storePath;
    },
    async saveSessionStore(path, store) {
      stores.set(path, structuredClone(store));
    },
  };

  return {
    calls,
    sdk,
    storePath,
  };
}

describe('openclaw conversation runner', () => {
  it('returns a plain-text response and persists Trigger-owned session state', async () => {
    const deps = createDependencies('openclaw-runner-text-');
    const tempDir = join(deps.environment.stateDatabasePath, '..');
    const fake = createFakeOpenClawSdk({
      tempDir,
      response: 'Ship it.',
    });

    try {
      const result = await runOpenClawConversationRequest(
        buildRunRequest({
          workingDirectory: '/repo/worktree',
        }),
        deps,
        fake.sdk,
      );

      expect(result.response).toEqual({
        kind: 'text',
        text: 'Ship it.',
      });
      expect(result.selection).toEqual({
        profileName: 'CLAUDE_CODE.REVIEW',
        engineModel: 'anthropic.claude-sonnet-4-5',
        modelRef: 'anthropic/claude-sonnet-4-5',
      });
      expect(result.run.agentRunId).toBe('run-1');
      expect(result.run.selectedModel).toEqual({
        provider: 'anthropic',
        model: 'claude-sonnet-4-5',
        thinkingLevel: 'medium',
      });
      expect(fake.calls.lastConfig?.agents).toEqual({
        defaults: {
          model: 'anthropic/claude-sonnet-4-5',
          workspace: '/repo/worktree',
        },
      });
      expect(
        deps.stateStore.getOpenClawConversationSession('corr-123'),
      ).toMatchObject({
        correlationKey: 'corr-123',
        sessionId: 'session-1',
        status: 'active',
        workingDirectory: '/repo/worktree',
      });
    } finally {
      deps.stateStore.close();
    }
  });

  it('supports structured-output mode with semantic operation-key profile selection', async () => {
    const deps = createDependencies('openclaw-runner-structured-');
    const tempDir = join(deps.environment.stateDatabasePath, '..');
    const fake = createFakeOpenClawSdk({
      tempDir,
      response: '```json\n{"needs_attention":false,"reasoning":"Looks complete."}\n```',
    });

    try {
      const result = await runOpenClawConversationRequest(
        buildRunRequest({
          response: {
            kind: 'structured',
            schema: {
              type: 'object',
              properties: {
                needs_attention: { type: 'boolean' },
                reasoning: { type: 'string', minLength: 1 },
              },
              required: ['needs_attention', 'reasoning'],
              additionalProperties: false,
            },
          },
          selection: {
            operationKey: TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
          },
        }),
        deps,
        fake.sdk,
      );

      expect(result.response).toEqual({
        kind: 'structured',
        text: '```json\n{"needs_attention":false,"reasoning":"Looks complete."}\n```',
        value: {
          needs_attention: false,
          reasoning: 'Looks complete.',
        },
      });
      expect(result.selection.profileName).toBe('CLAUDE_CODE.REVIEW');
    } finally {
      deps.stateStore.close();
    }
  });

  it('propagates timeout failures and records them in Trigger-owned idempotency state', async () => {
    const deps = createDependencies('openclaw-runner-timeout-');
    const tempDir = join(deps.environment.stateDatabasePath, '..');
    const fake = createFakeOpenClawSdk({
      tempDir,
      onGetReply: async ({ opts }) => {
        const abortSignal = opts.abortSignal as AbortSignal;
        await new Promise((_, reject) => {
          abortSignal.addEventListener(
            'abort',
            () => reject(Object.assign(new Error('aborted'), { name: 'AbortError' })),
            { once: true },
          );
        });
      },
    });

    try {
      await expect(
        runOpenClawConversationRequest(
          buildRunRequest({
            timeoutMs: 25,
          }),
          deps,
          fake.sdk,
        ),
      ).rejects.toThrow('timed out after 25ms');

      expect(
        deps.stateStore.getClaim('openclaw-conversation:idem-123'),
      ).toMatchObject({
        status: 'failed',
        error: 'OpenClaw conversation timed out after 25ms.',
      });
    } finally {
      deps.stateStore.close();
    }
  });

  it('cleans up persisted OpenClaw session state explicitly', async () => {
    const deps = createDependencies('openclaw-runner-cleanup-');
    const tempDir = join(deps.environment.stateDatabasePath, '..');
    const fake = createFakeOpenClawSdk({
      tempDir,
      response: 'Cleanup me.',
    });

    try {
      const first = await runOpenClawConversationRequest(
        buildRunRequest(),
        deps,
        fake.sdk,
      );
      const transcriptPath = join(tempDir, `${first.session.key}.jsonl`);

      expect(Bun.file(transcriptPath).size).toBeGreaterThan(0);

      const cleanup = await cleanupOpenClawConversationRequest(
        {
          action: 'cleanup',
          correlation: buildRunRequest().correlation,
          session: {
            key: first.session.key,
          },
        },
        deps,
        fake.sdk,
      );

      expect(cleanup.disposition).toBe('cleaned');
      expect(
        deps.stateStore.getOpenClawConversationSession('corr-123'),
      ).toMatchObject({
        status: 'cleaned',
      });
      expect(await Bun.file(transcriptPath).exists()).toBe(false);

      const repeatCleanup = await cleanupOpenClawConversationRequest(
        {
          action: 'cleanup',
          correlation: buildRunRequest().correlation,
          session: {
            key: first.session.key,
          },
        },
        deps,
        fake.sdk,
      );

      expect(repeatCleanup.disposition).toBe('already-cleaned');
    } finally {
      deps.stateStore.close();
    }
  });

  it('returns the stored result on idempotent re-entry without re-running OpenClaw', async () => {
    const deps = createDependencies('openclaw-runner-idempotent-');
    const tempDir = join(deps.environment.stateDatabasePath, '..');
    const fake = createFakeOpenClawSdk({
      tempDir,
      response: 'Only once.',
    });

    try {
      const request = buildRunRequest();
      const first = await runOpenClawConversationRequest(request, deps, fake.sdk);
      const second = await runOpenClawConversationRequest(request, deps, fake.sdk);

      expect(first).toEqual(second);
      expect(fake.calls.getReply).toBe(1);
    } finally {
      deps.stateStore.close();
    }
  });

  it('wires the Trigger-owned runner to dedicated OpenClaw task ids', async () => {
    const previousSecret = process.env.TRIGGER_SECRET_KEY;
    process.env.TRIGGER_SECRET_KEY = 'tr_dev_test_key';

    const deps = createDependencies('openclaw-trigger-runner-');
    const calls: unknown[] = [];

    try {
      const runner = createTriggerOpenClawConversationRunner(
        deps,
        (async (...args: unknown[]) => {
          calls.push(args);
          return { id: `run-${calls.length}` };
        }) as never,
      );

      const runHandle = await runner.run(buildRunRequest());
      const cleanupHandle = await runner.cleanup({
        action: 'cleanup',
        correlation: buildRunRequest().correlation,
        session: {
          key: 'trigger:openclaw:Y29ycl8xMjM',
        },
      });

      expect(runHandle).toEqual({
        runId: 'run-1',
        taskId: OPENCLAW_CONVERSATION_TASK_ID,
      });
      expect(cleanupHandle).toEqual({
        runId: 'run-2',
        taskId: OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
      });
      expect(calls[0]).toEqual([
        OPENCLAW_CONVERSATION_TASK_ID,
        buildRunRequest(),
        {
          idempotencyKey: ['openclaw-conversation', 'run', 'idem-123'],
          concurrencyKey: 'openclaw:corr-123',
          tags: ['workflow:review-gate', 'scope:task-123', 'correlation:corr-123'],
        },
      ]);
      expect(calls[1]).toEqual([
        OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
        {
          action: 'cleanup',
          correlation: buildRunRequest().correlation,
          session: {
            key: 'trigger:openclaw:Y29ycl8xMjM',
          },
        },
        {
          idempotencyKey: ['openclaw-conversation', 'cleanup', 'idem-123'],
          concurrencyKey: 'openclaw:corr-123',
          tags: ['workflow:review-gate', 'scope:task-123', 'correlation:corr-123'],
        },
      ]);
    } finally {
      deps.stateStore.close();
      if (previousSecret === undefined) {
        delete process.env.TRIGGER_SECRET_KEY;
      } else {
        process.env.TRIGGER_SECRET_KEY = previousSecret;
      }
    }
  });
});
