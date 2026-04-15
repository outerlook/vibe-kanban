import { resolve } from 'node:path';

import { createSqliteStateStore } from '../state/sqlite-state-store';
import type { OrchestratorStateStore } from '../state/types';
import { VkRuntimeClient } from '../vk/runtime-client';
import type { VkApiConfig, VkMqttConfig } from '../vk/types';
import type { OrchestratorLogger } from './contracts';

export type RuntimeEnvironment = {
  vkApi: VkApiConfig;
  vkMqtt: VkMqttConfig;
  stateDatabasePath: string;
  schemaVersion: string;
  eventTypes?: string[];
  codeRabbitPollScopeKey: string;
  mqttRouterMode: 'trigger' | 'direct';
};

export type RuntimeDependencies = {
  environment: RuntimeEnvironment;
  stateStore: OrchestratorStateStore;
  vkClient: VkRuntimeClient;
  logger: OrchestratorLogger;
};

export function createConsoleLogger(): OrchestratorLogger {
  function write(level: 'debug' | 'info' | 'warn' | 'error', message: string, context?: unknown) {
    const prefix = `[trigger-vk:${level}]`;
    if (context === undefined) {
      console[level](prefix, message);
      return;
    }

    console[level](prefix, message, JSON.stringify(context));
  }

  return {
    debug(message, context) {
      write('debug', message, context);
    },
    info(message, context) {
      write('info', message, context);
    },
    warn(message, context) {
      write('warn', message, context);
    },
    error(message, context) {
      write('error', message, context);
    },
  };
}

function readRequiredEnv(name: string, env: NodeJS.ProcessEnv): string {
  const value = env[name]?.trim();
  if (!value) {
    throw new Error(`Missing required environment variable ${name}`);
  }

  return value;
}

function readOptionalEnv(name: string, env: NodeJS.ProcessEnv): string | undefined {
  const value = env[name]?.trim();
  return value ? value : undefined;
}

export function createRuntimeEnvironment(env: NodeJS.ProcessEnv = process.env): RuntimeEnvironment {
  const authMode =
    (readOptionalEnv('VK_API_AUTH_MODE', env) ?? 'none') as VkApiConfig['authMode'];
  const token = readOptionalEnv('VK_API_TOKEN', env);
  const headerName = readOptionalEnv('VK_API_HEADER_NAME', env);
  const headerPrefix = readOptionalEnv('VK_API_HEADER_PREFIX', env);
  const username = readOptionalEnv('VK_MQTT_USERNAME', env);
  const password = readOptionalEnv('VK_MQTT_PASSWORD', env);
  const clientId = readOptionalEnv('VK_MQTT_CLIENT_ID', env);
  const clean = readOptionalEnv('VK_MQTT_CLEAN', env)
    ? readOptionalEnv('VK_MQTT_CLEAN', env) === 'true'
    : undefined;

  const eventTypes = env.VK_ORCHESTRATION_EVENT_TYPES
    ? env.VK_ORCHESTRATION_EVENT_TYPES.split(',').map((entry) => entry.trim()).filter(Boolean)
    : undefined;

  return {
    vkApi: {
      baseUrl: readRequiredEnv('VK_API_BASE_URL', env),
      authMode,
      ...(token ? { token } : {}),
      ...(headerName ? { headerName } : {}),
      ...(headerPrefix ? { headerPrefix } : {}),
    },
    vkMqtt: {
      brokerUrl: readRequiredEnv('VK_MQTT_BROKER_URL', env),
      topicNamespace: readRequiredEnv('VK_MQTT_TOPIC_NAMESPACE', env),
      ...(username ? { username } : {}),
      ...(password ? { password } : {}),
      ...(clientId ? { clientId } : {}),
      ...(clean === undefined ? {} : { clean }),
    },
    stateDatabasePath: resolve(
      env.VK_TRIGGER_STATE_DB ?? 'integrations/trigger-vibe-kanban/data/orchestrator-state.sqlite',
    ),
    schemaVersion: env.VK_ORCHESTRATION_SCHEMA_VERSION ?? 'vk_orchestration_v1',
    ...(eventTypes ? { eventTypes } : {}),
    codeRabbitPollScopeKey: env.CODERABBIT_POLL_SCOPE_KEY ?? 'global',
    mqttRouterMode:
      env.VK_MQTT_ROUTER_MODE === 'direct' ? 'direct' : 'trigger',
  };
}

export function createRuntimeDependencies(
  environment = createRuntimeEnvironment(),
  logger = createConsoleLogger(),
): RuntimeDependencies {
  return {
    environment,
    stateStore: createSqliteStateStore(environment.stateDatabasePath),
    vkClient: new VkRuntimeClient(environment.vkApi),
    logger,
  };
}
