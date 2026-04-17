import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { BaseCodingAgent, type ExecutorProfileId } from '../../../../shared/types';

import { readRequiredEnv } from './env';

export const VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR =
  'VK_TRIGGER_EXECUTOR_CONFIG_PATH';

export const TRIGGER_EXECUTOR_OPERATION_KEYS = {
  lifecycleAutopilotStartTaskExecution:
    'lifecycle:autopilot-start-task-execution',
  reviewGateCreateConversation: 'review-gate:create-conversation',
  reviewGateGenerateCommitMessage: 'review-gate:generate-commit-message',
} as const;

export type TriggerExecutorOperationKey =
  (typeof TRIGGER_EXECUTOR_OPERATION_KEYS)[keyof typeof TRIGGER_EXECUTOR_OPERATION_KEYS];

export type TriggerExecutorProfileMap = Record<
  TriggerExecutorOperationKey,
  ExecutorProfileId
>;

export type TriggerExecutorMapping = {
  configPath: string;
  resolve(operationKey: TriggerExecutorOperationKey): ExecutorProfileId;
};

const TRIGGER_EXECUTOR_OPERATION_KEY_SET = new Set<string>(
  Object.values(TRIGGER_EXECUTOR_OPERATION_KEYS),
);

const BASE_CODING_AGENT_SET = new Set<string>(Object.values(BaseCodingAgent));

function isTriggerExecutorOperationKey(
  value: string,
): value is TriggerExecutorOperationKey {
  return TRIGGER_EXECUTOR_OPERATION_KEY_SET.has(value);
}

function parseExecutorProfileId(
  value: string,
  sourcePath: string,
  lineNumber: number,
  operationKey: TriggerExecutorOperationKey,
): ExecutorProfileId {
  const parts = value.split('.');
  const executor = parts[0]?.trim();
  const variant = parts[1]?.trim();

  if (
    parts.length !== 2 ||
    !executor ||
    !variant ||
    parts.some((part) => part.trim().length === 0)
  ) {
    throw new Error(
      `Invalid Trigger executor profile for ${operationKey} at ${sourcePath}:${lineNumber}; expected <PROFILE>.<VARIANT>.`,
    );
  }

  if (!BASE_CODING_AGENT_SET.has(executor)) {
    throw new Error(
      `Invalid Trigger executor profile for ${operationKey} at ${sourcePath}:${lineNumber}; unknown profile ${executor}.`,
    );
  }

  return {
    executor: executor as BaseCodingAgent,
    variant,
  };
}

function parseTriggerExecutorProfileMap(
  contents: string,
  sourcePath: string,
): TriggerExecutorProfileMap {
  const profiles = {} as Partial<TriggerExecutorProfileMap>;

  // Keep this line-oriented instead of JSON so duplicate keys fail fast rather
  // than being silently overwritten during parse.
  for (const [index, rawLine] of contents.split(/\r?\n/).entries()) {
    const lineNumber = index + 1;
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith('#')) {
      continue;
    }

    const separatorIndex = rawLine.indexOf('=');
    if (separatorIndex <= 0 || separatorIndex === rawLine.length - 1) {
      throw new Error(
        `Invalid Trigger executor config line at ${sourcePath}:${lineNumber}; expected <OPERATION_KEY>=<PROFILE>.<VARIANT>.`,
      );
    }

    const rawKey = rawLine.slice(0, separatorIndex).trim();
    const rawValue = rawLine.slice(separatorIndex + 1).trim();

    if (!isTriggerExecutorOperationKey(rawKey)) {
      throw new Error(
        `Unknown Trigger executor operation key ${rawKey} at ${sourcePath}:${lineNumber}.`,
      );
    }

    if (profiles[rawKey]) {
      throw new Error(
        `Duplicate Trigger executor operation key ${rawKey} at ${sourcePath}:${lineNumber}.`,
      );
    }

    profiles[rawKey] = parseExecutorProfileId(
      rawValue,
      sourcePath,
      lineNumber,
      rawKey,
    );
  }

  const missingOperationKeys = Object.values(TRIGGER_EXECUTOR_OPERATION_KEYS).filter(
    (operationKey) => !profiles[operationKey],
  );

  if (missingOperationKeys.length > 0) {
    throw new Error(
      `Missing Trigger executor mappings in ${sourcePath}: ${missingOperationKeys.join(', ')}.`,
    );
  }

  return profiles as TriggerExecutorProfileMap;
}

export function createTriggerExecutorMapping(
  profiles: TriggerExecutorProfileMap,
  configPath: string,
): TriggerExecutorMapping {
  return {
    configPath,
    resolve(operationKey) {
      const profile = profiles[operationKey];
      if (!profile) {
        throw new Error(
          `Missing Trigger executor mapping for operation ${operationKey}.`,
        );
      }

      return { ...profile };
    },
  };
}

export function loadTriggerExecutorMapping(
  configPath: string,
): TriggerExecutorMapping {
  const resolvedConfigPath = resolve(configPath);

  let contents: string;
  try {
    contents = readFileSync(resolvedConfigPath, 'utf8');
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(
      `Failed to read Trigger executor config at ${resolvedConfigPath}: ${message}`,
    );
  }

  return createTriggerExecutorMapping(
    parseTriggerExecutorProfileMap(contents, resolvedConfigPath),
    resolvedConfigPath,
  );
}

export function loadTriggerExecutorMappingFromEnv(
  env: NodeJS.ProcessEnv = process.env,
): TriggerExecutorMapping {
  return loadTriggerExecutorMapping(
    readRequiredEnv(VK_TRIGGER_EXECUTOR_CONFIG_PATH_ENV_VAR, env),
  );
}
