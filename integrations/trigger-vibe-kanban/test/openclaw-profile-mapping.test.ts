import { afterEach, describe, expect, it } from 'bun:test';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { createRuntimeEnvironment } from '../src/runtime/dependencies';
import { TRIGGER_EXECUTOR_OPERATION_KEYS } from '../src/runtime/executor-mapping';
import {
  VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR,
} from '../src/runtime/executor-mapping';
import {
  VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR,
  createOpenClawSessionConfigResolver,
  createTriggerOpenClawProfileMapping,
} from '../src/runtime/openclaw-profile-mapping';
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

function createBaseEnv(overrides: NodeJS.ProcessEnv = {}): NodeJS.ProcessEnv {
  return {
    VK_API_BASE_URL: 'http://127.0.0.1:3000',
    VK_MQTT_BROKER_URL: 'mqtt://127.0.0.1:1883',
    VK_MQTT_TOPIC_NAMESPACE: 'vk/orchestration',
    [VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR]: writeExecutorConfig([
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution}=CODEX.DEFAULT`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation}=CLAUDE_CODE.REVIEW`,
      `${TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage}=CLAUDE_CODE.COMMIT`,
    ].join('\n')),
    ...overrides,
  };
}

describe('trigger openclaw profile mapping', () => {
  it('fails fast when the OpenClaw config path env var is missing', () => {
    expect(() => createRuntimeEnvironment(createBaseEnv())).toThrow(
      `Missing required environment variable ${VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR}`,
    );
  });

  it('resolves reviewer and commit-helper profile names to deterministic engine.model values', () => {
    const environment = createRuntimeEnvironment(
      createBaseEnv({
        [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig([
          'CODEX.DEFAULT=openai.gpt-5',
          'CLAUDE_CODE.REVIEW=anthropic.claude-sonnet-4-5',
          'CLAUDE_CODE.COMMIT=openai.gpt-5-mini',
        ].join('\n')),
      }),
    );

    expect(
      environment.openClawSessionConfigResolver.resolve(
        environment.triggerExecutorMapping.resolveProfileName(
          TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateCreateConversation,
        ),
      ),
    ).toEqual({
      engine: {
        model: 'anthropic.claude-sonnet-4-5',
      },
    });

    expect(
      environment.openClawSessionConfigResolver.resolve(
        environment.triggerExecutorMapping.resolveProfileName(
          TRIGGER_EXECUTOR_OPERATION_KEYS.reviewGateGenerateCommitMessage,
        ),
      ),
    ).toEqual({
      engine: {
        model: 'openai.gpt-5-mini',
      },
    });
  });

  it('fails fast on malformed engine.model entries', () => {
    expect(() =>
      createRuntimeEnvironment(
        createBaseEnv({
          [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig([
            'CODEX.DEFAULT=openai.gpt-5',
            'CLAUDE_CODE.REVIEW=anthropic',
            'CLAUDE_CODE.COMMIT=openai.gpt-5-mini',
          ].join('\n')),
        }),
      )).toThrow('expected <ENGINE>.<MODEL>');
  });

  it('fails fast when a selected Trigger profile name has no OpenClaw mapping', () => {
    expect(() =>
      createRuntimeEnvironment(
        createBaseEnv({
          [VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR]: writeOpenClawConfig([
            'CODEX.DEFAULT=openai.gpt-5',
            'CLAUDE_CODE.REVIEW=anthropic.claude-sonnet-4-5',
          ].join('\n')),
        }),
      )).toThrow('Unknown Trigger OpenClaw profile name CLAUDE_CODE.COMMIT');
  });

  it('fails explicitly when resolving an unknown profile name', () => {
    const resolver = createOpenClawSessionConfigResolver(
      createTriggerOpenClawProfileMapping(
        {
          'CLAUDE_CODE.REVIEW': 'anthropic.claude-sonnet-4-5',
        },
        '<inline>',
      ),
    );

    expect(() => resolver.resolve('CLAUDE_CODE.COMMIT')).toThrow(
      'Unknown Trigger OpenClaw profile name CLAUDE_CODE.COMMIT',
    );
  });
});
