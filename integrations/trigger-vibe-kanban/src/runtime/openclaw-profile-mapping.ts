import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { readRequiredEnv } from './env';

export const VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR =
  'VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH';

export type TriggerOpenClawProfileName = string;

export type OpenClawEngineModel = string;

export type OpenClawSessionConfig = {
  engine: {
    model: OpenClawEngineModel;
  };
};

export type TriggerOpenClawProfileMap = Record<
  TriggerOpenClawProfileName,
  OpenClawEngineModel
>;

export type TriggerOpenClawProfileMapping = {
  configPath: string;
  resolveEngineModel(profileName: TriggerOpenClawProfileName): OpenClawEngineModel;
};

export type OpenClawSessionConfigResolver = {
  resolve(profileName: TriggerOpenClawProfileName): OpenClawSessionConfig;
};

function parseProfileName(
  value: string,
  sourcePath: string,
  lineNumber: number,
): TriggerOpenClawProfileName {
  const profileName = value.trim();

  if (!profileName || /\s/.test(profileName)) {
    throw new Error(
      `Invalid Trigger OpenClaw profile name at ${sourcePath}:${lineNumber}; expected a non-empty profile name without whitespace.`,
    );
  }

  return profileName;
}

function parseOpenClawEngineModel(
  value: string,
  sourcePath: string,
  lineNumber: number,
  profileName: TriggerOpenClawProfileName,
): OpenClawEngineModel {
  const engineModel = value.trim();
  const separatorIndex = engineModel.indexOf('.');
  const engine = separatorIndex > 0 ? engineModel.slice(0, separatorIndex).trim() : '';
  const model = separatorIndex > 0 ? engineModel.slice(separatorIndex + 1).trim() : '';

  if (!engine || !model || /\s/.test(engine) || /\s/.test(model)) {
    throw new Error(
      `Invalid Trigger OpenClaw engine.model for ${profileName} at ${sourcePath}:${lineNumber}; expected <ENGINE>.<MODEL>.`,
    );
  }

  return `${engine}.${model}`;
}

function parseTriggerOpenClawProfileMap(
  contents: string,
  sourcePath: string,
): TriggerOpenClawProfileMap {
  const profiles: TriggerOpenClawProfileMap = {};

  for (const [index, rawLine] of contents.split(/\r?\n/).entries()) {
    const lineNumber = index + 1;
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith('#')) {
      continue;
    }

    const separatorIndex = rawLine.indexOf('=');
    if (separatorIndex <= 0 || separatorIndex === rawLine.length - 1) {
      throw new Error(
        `Invalid Trigger OpenClaw config line at ${sourcePath}:${lineNumber}; expected <PROFILE_NAME>=<ENGINE>.<MODEL>.`,
      );
    }

    const profileName = parseProfileName(
      rawLine.slice(0, separatorIndex),
      sourcePath,
      lineNumber,
    );

    if (profiles[profileName]) {
      throw new Error(
        `Duplicate Trigger OpenClaw profile name ${profileName} at ${sourcePath}:${lineNumber}.`,
      );
    }

    profiles[profileName] = parseOpenClawEngineModel(
      rawLine.slice(separatorIndex + 1),
      sourcePath,
      lineNumber,
      profileName,
    );
  }

  return profiles;
}

export function createTriggerOpenClawProfileMapping(
  profiles: TriggerOpenClawProfileMap,
  configPath: string,
): TriggerOpenClawProfileMapping {
  const engineModels = new Map<TriggerOpenClawProfileName, OpenClawEngineModel>();

  for (const [profileName, engineModel] of Object.entries(profiles)) {
    const normalizedProfileName = parseProfileName(profileName, configPath, 0);
    engineModels.set(
      normalizedProfileName,
      parseOpenClawEngineModel(engineModel, configPath, 0, normalizedProfileName),
    );
  }

  return {
    configPath,
    resolveEngineModel(profileName) {
      const engineModel = engineModels.get(profileName);
      if (!engineModel) {
        throw new Error(
          `Unknown Trigger OpenClaw profile name ${profileName} in ${configPath}.`,
        );
      }

      return engineModel;
    },
  };
}

export function createOpenClawSessionConfigResolver(
  profileMapping: TriggerOpenClawProfileMapping,
): OpenClawSessionConfigResolver {
  return {
    resolve(profileName) {
      return {
        engine: {
          model: profileMapping.resolveEngineModel(profileName),
        },
      };
    },
  };
}

export function loadTriggerOpenClawProfileMapping(
  configPath: string,
): TriggerOpenClawProfileMapping {
  const resolvedConfigPath = resolve(configPath);

  let contents: string;
  try {
    contents = readFileSync(resolvedConfigPath, 'utf8');
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(
      `Failed to read Trigger OpenClaw config at ${resolvedConfigPath}: ${message}`,
    );
  }

  return createTriggerOpenClawProfileMapping(
    parseTriggerOpenClawProfileMap(contents, resolvedConfigPath),
    resolvedConfigPath,
  );
}

export function loadTriggerOpenClawProfileMappingFromEnv(
  env: NodeJS.ProcessEnv = process.env,
): TriggerOpenClawProfileMapping {
  return loadTriggerOpenClawProfileMapping(
    readRequiredEnv(VK_TRIGGER_OPENCLAW_PROFILE_CONFIG_PATH_ENV_VAR, env),
  );
}
