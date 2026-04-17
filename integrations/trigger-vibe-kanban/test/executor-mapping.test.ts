import { afterEach, describe, expect, it } from 'bun:test';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { BaseCodingAgent } from '../../../shared/types';

import { createRuntimeEnvironment } from '../src/runtime/dependencies';
import {
  TRIGGER_EXECUTOR_OPERATION_KEYS,
  VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR,
  createTriggerExecutorMapping,
} from '../src/runtime/executor-mapping';
import { createTempDir, removeTempDir } from './helpers';

const tempDirs: string[] = [];

afterEach(() => {
  while (tempDirs.length > 0) {
    const next = tempDirs.pop();
    if (next) {
      removeTempDir(next);
    }
  }
});

function createBaseEnv(overrides: NodeJS.ProcessEnv = {}): NodeJS.ProcessEnv {
  return {
    VK_API_BASE_URL: 'http://127.0.0.1:3000',
    VK_MQTT_BROKER_URL: 'mqtt://127.0.0.1:1883',
    VK_MQTT_TOPIC_NAMESPACE: 'vk/orchestration',
    ...overrides,
  };
}

function writeExecutorConfig(contents: string): string {
  const tempDir = createTempDir('trigger-executor-config-');
  tempDirs.push(tempDir);

  const configPath = join(tempDir, 'trigger-executors.conf');
  writeFileSync(configPath, contents);
  return configPath;
}

function createValidExecutorConfig(): string {
  return [
    `${TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution}=CODEX.DEFAULT`,
    `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation}=CLAUDE_CODE.REVIEW`,
    `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage}=CLAUDE_CODE.COMMIT`,
  ].join('\n');
}

describe('trigger executor mapping', () => {
  it('fails fast when the executor config path env var is missing', () => {
    expect(() => createRuntimeEnvironment(createBaseEnv())).toThrow(
      `Missing required environment variable ${VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR}`,
    );
  });

  it('fails fast when the executor config file does not exist', () => {
    expect(() =>
      createRuntimeEnvironment(
        createBaseEnv({
          [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: join(
            process.cwd(),
            'missing-trigger-executors.conf',
          ),
        }),
      )).toThrow('Failed to read Trigger executor config');
  });

  it('fails fast on malformed dotted executor profile values', () => {
    const configPath = writeExecutorConfig([
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution}=CODEX.DEFAULT`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation}=CLAUDE_CODE`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage}=CLAUDE_CODE.COMMIT`,
    ].join('\n'));

    expect(() =>
      createRuntimeEnvironment(
        createBaseEnv({
          [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: configPath,
        }),
      )).toThrow('expected <PROFILE>.<VARIANT>');
  });

  it('fails fast on duplicate operation keys', () => {
    const configPath = writeExecutorConfig([
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution}=CODEX.DEFAULT`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation}=CLAUDE_CODE.REVIEW`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation}=CLAUDE_CODE.SECONDARY`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage}=CLAUDE_CODE.COMMIT`,
    ].join('\n'));

    expect(() =>
      createRuntimeEnvironment(
        createBaseEnv({
          [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: configPath,
        }),
      )).toThrow('Duplicate Trigger executor operation key');
  });

  it('fails fast on unknown operation keys', () => {
    const configPath = writeExecutorConfig([
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution}=CODEX.DEFAULT`,
      'review-gate:unknown-step=CLAUDE_CODE.REVIEW',
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage}=CLAUDE_CODE.COMMIT`,
    ].join('\n'));

    expect(() =>
      createRuntimeEnvironment(
        createBaseEnv({
          [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: configPath,
        }),
      )).toThrow('Unknown Trigger executor operation key');
  });

  it('fails fast when a required operation entry is missing', () => {
    const configPath = writeExecutorConfig([
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution}=CODEX.DEFAULT`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation}=CLAUDE_CODE.REVIEW`,
    ].join('\n'));

    expect(() =>
      createRuntimeEnvironment(
        createBaseEnv({
          [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: configPath,
        }),
      )).toThrow(
      TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage,
    );
  });

  it('resolves distinct executor profiles for semantic operation keys', () => {
    const configPath = writeExecutorConfig(createValidExecutorConfig());
    const environment = createRuntimeEnvironment(
      createBaseEnv({
        [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: configPath,
      }),
    );

    expect(
      environment.triggerExecutorMapping.resolve(
        TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution,
      ),
    ).toEqual({
      executor: BaseCodingAgent.CODEX,
      variant: 'DEFAULT',
    });
    expect(
      environment.triggerExecutorMapping.resolve(
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
      ),
    ).toEqual({
      executor: BaseCodingAgent.CLAUDE_CODE,
      variant: 'REVIEW',
    });
    expect(
      environment.triggerExecutorMapping.resolve(
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage,
      ),
    ).toEqual({
      executor: BaseCodingAgent.CLAUDE_CODE,
      variant: 'COMMIT',
    });
  });

  it('fails fast when resolving an operation without a configured profile', () => {
    const mapping = createTriggerExecutorMapping(
      {
        [TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution]: {
          executor: BaseCodingAgent.CODEX,
          variant: 'DEFAULT',
        },
      } as unknown as Parameters<typeof createTriggerExecutorMapping>[0],
      '<inline>',
    );

    expect(() =>
      mapping.resolve(TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation),
    ).toThrow('Missing Trigger executor mapping for operation');
  });
});
