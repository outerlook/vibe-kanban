import { once } from 'node:events';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import net from 'node:net';

import { Aedes } from 'aedes';

import { BaseCodingAgent } from '../../../shared/types';

import {
  createTriggerExecutorMapping,
  TRIGGER_EXECUTOR_OPERATION_KEYS,
} from '../src/runtime/executor-mapping';
import {
  createOpenClawSessionConfigResolver,
  createTriggerOpenClawProfileMapping,
} from '../src/runtime/openclaw-profile-mapping';

export function listen(server: net.Server | ReturnType<typeof createServer>) {
  return new Promise<number>((resolve, reject) => {
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (address && typeof address === 'object') {
        resolve(address.port);
        return;
      }

      reject(new Error('server did not expose a port'));
    });
  });
}

export async function createBroker() {
  const broker = await Aedes.createBroker();
  const server = net.createServer(broker.handle);
  const port = await listen(server);

  return {
    broker,
    server,
    port,
    async close() {
      await new Promise<void>((resolve) => broker.close(() => resolve()));
      await new Promise<void>((resolve) => server.close(() => resolve()));
    },
  };
}

export async function createJsonServer(
  handler: (request: IncomingMessage, response: ServerResponse) => Promise<void> | void,
) {
  const server = createServer((request, response) => void handler(request, response));
  const port = await listen(server);

  return {
    server,
    port,
    async close() {
      server.close();
      await once(server, 'close');
    },
  };
}

export function createTempDir(prefix: string): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

export function removeTempDir(path: string): void {
  rmSync(path, { recursive: true, force: true });
}

export function createTestTriggerExecutorMapping() {
  return createTriggerExecutorMapping(
    {
      [TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution]: {
        executor: BaseCodingAgent.CODEX,
        variant: 'DEFAULT',
      },
      [TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation]: {
        executor: BaseCodingAgent.CLAUDE_CODE,
        variant: 'REVIEW',
      },
      [TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage]: {
        executor: BaseCodingAgent.CLAUDE_CODE,
        variant: 'COMMIT',
      },
    },
    '<test>',
  );
}

export function createTestOpenClawSessionConfigResolver() {
  return createOpenClawSessionConfigResolver(
    createTriggerOpenClawProfileMapping(
      {
        'CODEX.DEFAULT': 'openai.gpt-5',
        'CLAUDE_CODE.REVIEW': 'anthropic.claude-sonnet-4-5',
        'CLAUDE_CODE.COMMIT': 'anthropic.claude-sonnet-4-5',
      },
      '<test>',
    ),
  );
}
