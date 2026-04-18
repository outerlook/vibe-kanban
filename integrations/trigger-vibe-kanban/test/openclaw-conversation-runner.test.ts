import { afterEach, describe, expect, it } from 'bun:test';
import { join } from 'node:path';

import {
  cleanupOpenClawConversationRequest,
  createSqliteStateStore,
  createTriggerOpenClawConversationRunner,
  OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
  OPENCLAW_CONVERSATION_TASK_ID,
  runOpenClawConversationRequest,
  type OpenClawConversationRunRequest,
  type OpenClawSessionManager,
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

type FakeSessionRecord = {
  name: string;
  sessionId: string;
  cwd: string;
  model: string | undefined;
  engine: string | undefined;
  created: string;
  active: boolean;
  persisted: boolean;
};

type FakeOpenClawHarness = {
  calls: {
    starts: Array<Record<string, unknown>>;
    messages: Array<Record<string, unknown>>;
    stops: string[];
  };
  manager: OpenClawSessionManager;
  unloadSession(name: string): void;
};

function createFakeOpenClawSessionManager(options: {
  response?: string;
  replyForMessage?: (input: { name: string; message: string }) => Promise<string> | string;
  sendError?: Error;
} = {}): FakeOpenClawHarness {
  const sessions = new Map<string, FakeSessionRecord>();
  const calls = {
    starts: [] as Array<Record<string, unknown>>,
    messages: [] as Array<Record<string, unknown>>,
    stops: [] as string[],
  };
  let sessionSequence = 0;

  function buildInfo(session: FakeSessionRecord) {
    return {
      name: session.name,
      claudeSessionId: session.sessionId,
      created: session.created,
      cwd: session.cwd,
      model: session.model,
      paused: false,
      stats: {
        turns: 0,
        toolCalls: 0,
        toolErrors: 0,
        tokensIn: 0,
        tokensOut: 0,
        cachedTokens: 0,
        costUsd: 0,
        isReady: true,
        startTime: session.created,
        lastActivity: session.created,
        contextPercent: 0,
        retries: 0,
      },
    };
  }

  const manager: OpenClawSessionManager = {
    async startSession(config) {
      const name = config.name ?? `session-${++sessionSequence}`;
      calls.starts.push({
        name,
        cwd: config.cwd,
        engine: config.engine,
        model: config.model,
      });

      const existing = sessions.get(name);
      if (existing) {
        existing.active = true;
        existing.persisted = true;
        existing.cwd = config.cwd ?? existing.cwd;
        existing.model = config.model ?? existing.model;
        existing.engine = config.engine ?? existing.engine;
        return buildInfo(existing) as Awaited<ReturnType<OpenClawSessionManager['startSession']>>;
      }

      const created = new Date().toISOString();
      const next: FakeSessionRecord = {
        name,
        sessionId: `session-${++sessionSequence}`,
        cwd: config.cwd ?? process.cwd(),
        model: config.model,
        engine: config.engine,
        created,
        active: true,
        persisted: true,
      };
      sessions.set(name, next);
      return buildInfo(next) as Awaited<ReturnType<OpenClawSessionManager['startSession']>>;
    },
    async sendMessage(name, message, requestOptions) {
      calls.messages.push({
        name,
        message,
        timeout: requestOptions?.timeout,
      });

      if (options.sendError) {
        throw options.sendError;
      }

      const session = sessions.get(name);
      if (!session || !session.active) {
        throw new Error(`Unknown active session ${name}`);
      }

      const output = options.replyForMessage
        ? await options.replyForMessage({ name, message })
        : options.response ?? 'Looks good to me.';
      requestOptions?.onChunk?.(output);
      return {
        output,
        sessionId: session.sessionId,
        events: [],
      };
    },
    async stopSession(name) {
      calls.stops.push(name);
      const session = sessions.get(name);
      if (!session || !session.active) {
        throw new Error(`Unknown active session ${name}`);
      }

      sessions.delete(name);
    },
    listSessions() {
      return Array.from(sessions.values())
        .filter((session) => session.active)
        .map((session) => buildInfo(session)) as ReturnType<OpenClawSessionManager['listSessions']>;
    },
    listPersistedSessions() {
      return Array.from(sessions.values())
        .filter((session) => session.persisted)
        .map((session) => ({ name: session.name }));
    },
  };

  return {
    calls,
    manager,
    unloadSession(name) {
      const session = sessions.get(name);
      if (session) {
        session.active = false;
      }
    },
  };
}

function deriveExpectedSessionKey(
  correlationKey: string,
  idempotencyKey: string,
): string {
  return [
    'trigger:openclaw',
    Buffer.from(correlationKey, 'utf8').toString('base64url'),
    Buffer.from(idempotencyKey, 'utf8').toString('base64url'),
  ].join(':');
}

describe('openclaw conversation runner', () => {
  it('resolves Trigger selection to standalone engine/model inputs and returns text output', async () => {
    const deps = createDependencies('openclaw-runner-text-');
    const fake = createFakeOpenClawSessionManager({
      response: 'Looks good to me.',
    });

    try {
      const result = await runOpenClawConversationRequest(
        buildRunRequest({
          selection: {
            operationKey: TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
          },
        }),
        deps,
        fake.manager,
      );

      expect(result.response).toEqual({
        kind: 'text',
        text: 'Looks good to me.',
      });
      expect(result.selection).toEqual({
        profileName: 'CLAUDE_CODE.REVIEW',
        engineModel: 'anthropic.claude-sonnet-4-5',
        modelRef: 'anthropic/claude-sonnet-4-5',
      });
      expect(result.run.agentRunId).toBeNull();
      expect(result.run.selectedModel).toEqual({
        provider: 'anthropic',
        model: 'claude-sonnet-4-5',
      });
      expect(result.session).toEqual({
        key: deriveExpectedSessionKey('corr-123', 'idem-123'),
        id: 'session-1',
        cleanedUp: false,
      });
      expect(fake.calls.starts).toEqual([
        {
          name: deriveExpectedSessionKey('corr-123', 'idem-123'),
          cwd: '/tmp/worktree',
          engine: 'claude',
          model: 'claude-sonnet-4-5',
        },
      ]);
      expect(fake.calls.messages).toHaveLength(1);
      expect(
        deps.stateStore.getOpenClawConversationSession('corr-123', 'idem-123'),
      ).toMatchObject({
        correlationKey: 'corr-123',
        idempotencyKey: 'idem-123',
        sessionKey: deriveExpectedSessionKey('corr-123', 'idem-123'),
        sessionId: 'session-1',
        status: 'active',
      });
    } finally {
      deps.stateStore.close();
    }
  });

  it('parses and validates structured output', async () => {
    const deps = createDependencies('openclaw-runner-structured-');
    const fake = createFakeOpenClawSessionManager({
      response: JSON.stringify({ summary: 'Ship it', approved: true }),
    });

    try {
      const result = await runOpenClawConversationRequest(
        buildRunRequest({
          response: {
            kind: 'structured',
            schema: {
              type: 'object',
              required: ['summary', 'approved'],
              properties: {
                summary: { type: 'string', minLength: 1 },
                approved: { type: 'boolean' },
              },
            },
          },
        }),
        deps,
        fake.manager,
      );

      expect(result.response).toEqual({
        kind: 'structured',
        text: JSON.stringify({ summary: 'Ship it', approved: true }),
        value: { summary: 'Ship it', approved: true },
      });
    } finally {
      deps.stateStore.close();
    }
  });

  it('maps standalone timeout errors to the runner timeout contract', async () => {
    const deps = createDependencies('openclaw-runner-timeout-');
    const fake = createFakeOpenClawSessionManager({
      sendError: new Error('Timeout waiting for response'),
    });

    try {
      await expect(
        runOpenClawConversationRequest(
          buildRunRequest({
            timeoutMs: 25,
          }),
          deps,
          fake.manager,
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

  it('cleans up a persisted standalone session and keeps cleanup idempotent', async () => {
    const deps = createDependencies('openclaw-runner-cleanup-');
    const fake = createFakeOpenClawSessionManager({
      response: 'Cleanup me.',
    });

    try {
      const first = await runOpenClawConversationRequest(
        buildRunRequest(),
        deps,
        fake.manager,
      );

      fake.unloadSession(first.session.key);

      const cleanup = await cleanupOpenClawConversationRequest(
        {
          action: 'cleanup',
          correlation: buildRunRequest().correlation,
          session: {
            key: first.session.key,
          },
        },
        deps,
        fake.manager,
      );

      expect(cleanup.disposition).toBe('cleaned');
      expect(cleanup.session).toEqual({
        key: first.session.key,
        id: 'session-1',
        cleanedUp: true,
      });
      expect(fake.calls.starts.at(-1)).toEqual({
        name: first.session.key,
        cwd: '/tmp/worktree',
        engine: 'claude',
        model: 'claude-sonnet-4-5',
      });
      expect(fake.calls.stops).toEqual([first.session.key]);
      expect(
        deps.stateStore.getOpenClawConversationSession('corr-123', 'idem-123'),
      ).toMatchObject({
        status: 'cleaned',
      });

      const repeatCleanup = await cleanupOpenClawConversationRequest(
        {
          action: 'cleanup',
          correlation: buildRunRequest().correlation,
          session: {
            key: first.session.key,
          },
        },
        deps,
        fake.manager,
      );

      expect(repeatCleanup.disposition).toBe('already-cleaned');
    } finally {
      deps.stateStore.close();
    }
  });

  it('returns the stored result on idempotent re-entry without re-running OpenClaw', async () => {
    const deps = createDependencies('openclaw-runner-idempotent-');
    const fake = createFakeOpenClawSessionManager({
      response: 'Only once.',
    });

    try {
      const request = buildRunRequest();
      const first = await runOpenClawConversationRequest(request, deps, fake.manager);
      const second = await runOpenClawConversationRequest(request, deps, fake.manager);

      expect(first).toEqual(second);
      expect(fake.calls.messages).toHaveLength(1);
    } finally {
      deps.stateStore.close();
    }
  });

  it('uses distinct session identities for independent concurrent helper executions', async () => {
    const deps = createDependencies('openclaw-runner-concurrency-');
    const fake = createFakeOpenClawSessionManager({
      replyForMessage: async ({ name }) => `reply:${name}`,
    });

    try {
      const [first, second] = await Promise.all([
        runOpenClawConversationRequest(
          buildRunRequest({
            correlation: {
              workflowKey: 'review-gate',
              scopeKey: 'task-123',
              correlationKey: 'corr-123',
              idempotencyKey: 'idem-123',
            },
          }),
          deps,
          fake.manager,
        ),
        runOpenClawConversationRequest(
          buildRunRequest({
            correlation: {
              workflowKey: 'review-gate',
              scopeKey: 'task-123',
              correlationKey: 'corr-123',
              idempotencyKey: 'idem-456',
            },
          }),
          deps,
          fake.manager,
        ),
      ]);

      expect(first.session.key).not.toBe(second.session.key);
      expect(first.response).toEqual({
        kind: 'text',
        text: `reply:${first.session.key}`,
      });
      expect(second.response).toEqual({
        kind: 'text',
        text: `reply:${second.session.key}`,
      });
      expect(fake.calls.starts).toEqual(
        expect.arrayContaining([
          {
            name: first.session.key,
            cwd: '/tmp/worktree',
            engine: 'claude',
            model: 'claude-sonnet-4-5',
          },
          {
            name: second.session.key,
            cwd: '/tmp/worktree',
            engine: 'claude',
            model: 'claude-sonnet-4-5',
          },
        ]),
      );
      expect(
        deps.stateStore.getOpenClawConversationSession('corr-123', 'idem-123')?.sessionKey,
      ).toBe(first.session.key);
      expect(
        deps.stateStore.getOpenClawConversationSession('corr-123', 'idem-456')?.sessionKey,
      ).toBe(second.session.key);
    } finally {
      deps.stateStore.close();
    }
  });

  it('fails fast on unsupported standalone engine providers', async () => {
    const deps = createDependencies('openclaw-runner-engine-');
    const fake = createFakeOpenClawSessionManager();
    deps.environment.openClawSessionConfigResolver = {
      resolve() {
        return {
          engine: {
            model: 'mystery.experimental',
          },
        };
      },
    };

    try {
      await expect(
        runOpenClawConversationRequest(buildRunRequest(), deps, fake.manager),
      ).rejects.toThrow(
        "Resolved OpenClaw engine.model 'mystery.experimental' uses unsupported standalone engine 'mystery'.",
      );
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
          key: deriveExpectedSessionKey('corr-123', 'idem-123'),
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
            key: deriveExpectedSessionKey('corr-123', 'idem-123'),
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
