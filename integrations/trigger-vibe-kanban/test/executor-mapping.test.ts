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
import { VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR } from '../src/runtime/openclaw-profile-mapping';
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

function writeConfig(
  prefix: string,
  fileName: string,
  contents: string,
): string {
  const tempDir = createTempDir(prefix);
  tempDirs.push(tempDir);

  const configPath = join(tempDir, fileName);
  writeFileSync(configPath, contents);
  return configPath;
}

function writeExecutorConfig(contents: string): string {
  return writeConfig('trigger-executor-config-', 'trigger-executors.conf', contents);
}

function writeOpenClawConfig(contents: string): string {
  return writeConfig('trigger-openclaw-config-', 'trigger-openclaw.conf', contents);
}

function createValidExecutorConfig(): string {
  return [
    `${TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution}=CODEX.DEFAULT`,
    `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation}=CLAUDE_CODE.REVIEW`,
    `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage}=CLAUDE_CODE.COMMIT`,
  ].join('\n');
}

function createValidOpenClawConfig(): string {
  return [
    'CODEX.DEFAULT=openai.gpt-5',
    'CLAUDE_CODE.REVIEW=anthropic.claude-sonnet-4-5',
    'CLAUDE_CODE.COMMIT=anthropic.claude-sonnet-4-5',
  ].join('\n');
}

function createConfiguredEnv(overrides: NodeJS.ProcessEnv = {}): NodeJS.ProcessEnv {
  return createBaseEnv({
    [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: writeExecutorConfig(
      createValidExecutorConfig(),
    ),
    [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig(
      createValidOpenClawConfig(),
    ),
    ...overrides,
  });
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
          [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig(
            createValidOpenClawConfig(),
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
          [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig(
            createValidOpenClawConfig(),
          ),
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
          [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig(
            createValidOpenClawConfig(),
          ),
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
          [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig(
            createValidOpenClawConfig(),
          ),
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
          [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig(
            createValidOpenClawConfig(),
          ),
        }),
      )).toThrow(TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage);
  });

  it('resolves distinct executor profiles and preserves their selected profile names', () => {
    const environment = createRuntimeEnvironment(createConfiguredEnv());

    expect(
      environment.triggerExecutorMapping.resolve(
        TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution,
      ),
    ).toEqual({
      executor: BaseCodingAgent.CODEX,
      variant: 'DEFAULT',
    });
    expect(
      environment.triggerExecutorMapping.resolveProfileName(
        TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution,
      ),
    ).toBe('CODEX.DEFAULT');
    expect(
      environment.triggerExecutorMapping.resolve(
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
      ),
    ).toEqual({
      executor: BaseCodingAgent.CLAUDE_CODE,
      variant: 'REVIEW',
    });
    expect(
      environment.triggerExecutorMapping.resolveProfileName(
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
      ),
    ).toBe('CLAUDE_CODE.REVIEW');
    expect(
      environment.triggerExecutorMapping.resolve(
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage,
      ),
    ).toEqual({
      executor: BaseCodingAgent.CLAUDE_CODE,
      variant: 'COMMIT',
    });
    expect(
      environment.triggerExecutorMapping.resolveProfileName(
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage,
      ),
    ).toBe('CLAUDE_CODE.COMMIT');
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
    expect(() =>
      mapping.resolveProfileName(
        TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
      ),
    ).toThrow('Missing Trigger executor mapping for operation');
  });
});
