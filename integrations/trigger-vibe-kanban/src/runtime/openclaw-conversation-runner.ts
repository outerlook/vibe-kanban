import {
  SessionManager,
} from '@enderfga/openclaw-claude-code/dist/src/session-manager.js';
import type {
  EngineType,
  SendResult,
  SessionConfig,
  SessionInfo,
  StreamEvent,
} from '@enderfga/openclaw-claude-code/dist/src/types.js';
import type { Logger as OpenClawLogger } from '@enderfga/openclaw-claude-code/dist/src/logger.js';

import type { JsonValue } from '../state/types';
import type {
  OpenClawConversationCleanupRequest,
  OpenClawConversationCleanupResult,
  OpenClawConversationRequest,
  OpenClawConversationResponseMode,
  OpenClawConversationResult,
  OpenClawConversationRunRequest,
  OpenClawConversationRunResult,
} from './contracts';
import type { RuntimeDependencies } from './dependencies';

const OPENCLAW_CONVERSATION_WORKFLOW_KEY = 'openclaw/conversation';

type SelectedModel = {
  provider: string;
  model: string;
  thinkingLevel?: string;
};

type OpenClawSessionStartConfig = Partial<SessionConfig> & {
  name?: string;
};

export type OpenClawSessionManager = {
  startSession(config: OpenClawSessionStartConfig): Promise<SessionInfo>;
  sendMessage(
    name: string,
    message: string,
    options?: {
      timeout?: number;
      onChunk?: (chunk: string) => void;
      onEvent?: (event: StreamEvent) => void;
    },
  ): Promise<SendResult>;
  stopSession(name: string): Promise<void>;
  listSessions(): SessionInfo[];
  listPersistedSessions(): Array<{ name: string }>;
};

type ResolvedSessionSelection = {
  engine: EngineType;
  provider: string;
  model: string;
  modelRef: string;
};

let sharedOpenClawSessionManager: OpenClawSessionManager | null = null;

export class OpenClawConversationTimeoutError extends Error {
  readonly timeoutMs: number;

  constructor(timeoutMs: number) {
    super(`OpenClaw conversation timed out after ${timeoutMs}ms.`);
    this.name = 'OpenClawConversationTimeoutError';
    this.timeoutMs = timeoutMs;
  }
}

function isJsonRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function openClawConversationClaimKey(idempotencyKey: string): string {
  return `openclaw-conversation:${idempotencyKey}`;
}

function encodeSessionKeyPart(value: string): string {
  return Buffer.from(value, 'utf8').toString('base64url');
}

function deriveSessionKey(
  correlationKey: string,
  idempotencyKey: string,
): string {
  return [
    'trigger:openclaw',
    encodeSessionKeyPart(correlationKey),
    encodeSessionKeyPart(idempotencyKey),
  ].join(':');
}

function resolveRequestedProfileName(
  request: OpenClawConversationRunRequest,
  deps: RuntimeDependencies,
): string {
  if ('profileName' in request.selection) {
    return request.selection.profileName;
  }

  return deps.environment.triggerExecutorMapping.resolveProfileName(
    request.selection.operationKey,
  );
}

function mapEngineProviderToStandaloneEngine(
  provider: string,
  engineModel: string,
): EngineType {
  switch (provider.toLowerCase()) {
    case 'anthropic':
    case 'claude':
      return 'claude';
    case 'openai':
    case 'openai-codex':
    case 'codex':
      return 'codex';
    case 'google':
    case 'gemini':
      return 'gemini';
    case 'cursor':
      return 'cursor';
    default:
      throw new Error(
        `Resolved OpenClaw engine.model '${engineModel}' uses unsupported standalone engine '${provider}'.`,
      );
  }
}

function parseStandaloneSessionSelection(
  engineModel: string,
): ResolvedSessionSelection {
  const trimmed = engineModel.trim();
  if (trimmed.length === 0) {
    throw new Error('Resolved OpenClaw engine.model must not be empty.');
  }

  const separatorIndex = trimmed.indexOf('.');
  if (separatorIndex <= 0 || separatorIndex === trimmed.length - 1) {
    throw new Error(
      `Resolved OpenClaw engine.model '${trimmed}' must use <ENGINE>.<MODEL>.`,
    );
  }

  const provider = trimmed.slice(0, separatorIndex).trim();
  const model = trimmed.slice(separatorIndex + 1).trim();
  if (!provider || !model) {
    throw new Error(
      `Resolved OpenClaw engine.model '${trimmed}' must use <ENGINE>.<MODEL>.`,
    );
  }

  return {
    provider,
    engine: mapEngineProviderToStandaloneEngine(provider, trimmed),
    model,
    modelRef: `${provider}/${model}`,
  };
}

function sanitizeSessionKey(rawValue: string | undefined): string | null {
  const sessionKey = rawValue?.trim();
  return sessionKey ? sessionKey : null;
}

function resolveSessionKey(
  request: OpenClawConversationRunRequest,
  deps: RuntimeDependencies,
): string {
  const existing = deps.stateStore.getOpenClawConversationSession(
    request.correlation.correlationKey,
    request.correlation.idempotencyKey,
  );
  const explicitSessionKey = sanitizeSessionKey(request.session?.key);

  if (existing?.status === 'active') {
    if (explicitSessionKey && explicitSessionKey !== existing.sessionKey) {
      throw new Error(
        `OpenClaw conversation ${request.correlation.correlationKey}/${request.correlation.idempotencyKey} is already bound to session ${existing.sessionKey}.`,
      );
    }

    return existing.sessionKey;
  }

  const derivedSessionKey = deriveSessionKey(
    request.correlation.correlationKey,
    request.correlation.idempotencyKey,
  );
  if (explicitSessionKey && explicitSessionKey !== derivedSessionKey) {
    throw new Error(
      `OpenClaw conversation ${request.correlation.correlationKey}/${request.correlation.idempotencyKey} must use session ${derivedSessionKey}, not ${explicitSessionKey}.`,
    );
  }

  return derivedSessionKey;
}

function buildPrompt(request: OpenClawConversationRunRequest): string {
  const prompt = request.prompt.trim();
  if (prompt.length === 0) {
    throw new Error('OpenClaw conversation prompt must not be empty.');
  }

  if (request.response.kind === 'text') {
    return prompt;
  }

  return [
    prompt,
    '',
    'Return only valid JSON that matches this schema exactly.',
    'Do not wrap the JSON in markdown fences or add any explanatory text.',
    '',
    JSON.stringify(request.response.schema, null, 2),
  ].join('\n');
}

function normalizeStructuredText(text: string): string {
  const trimmed = text.trim();
  const fencedMatch = trimmed.match(/^```(?:json)?\s*([\s\S]*?)\s*```$/i);
  if (fencedMatch) {
    return fencedMatch[1]?.trim() ?? '';
  }

  return trimmed;
}

function parseStructuredResponse(text: string): JsonValue {
  const normalizedText = normalizeStructuredText(text);

  try {
    return JSON.parse(normalizedText) as JsonValue;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`OpenClaw structured response was not valid JSON: ${message}`);
  }
}

function stringifyComparableValue(value: unknown): string {
  return JSON.stringify(value);
}

function validateSchema(
  value: unknown,
  schema: JsonValue,
  path = '$',
): string | null {
  if (!isJsonRecord(schema)) {
    return `${path}: schema must be a JSON object.`;
  }

  const type = typeof schema.type === 'string' ? schema.type : undefined;

  if (Array.isArray(schema.enum)) {
    const matches = schema.enum.some(
      (candidate) => stringifyComparableValue(candidate) === stringifyComparableValue(value),
    );
    if (!matches) {
      return `${path}: value was not in the allowed enum set.`;
    }
  }

  switch (type) {
    case 'object': {
      if (!isJsonRecord(value)) {
        return `${path}: expected an object.`;
      }

      const properties = isJsonRecord(schema.properties)
        ? schema.properties
        : undefined;
      const required = Array.isArray(schema.required)
        ? schema.required.filter((entry): entry is string => typeof entry === 'string')
        : [];

      for (const key of required) {
        if (!(key in value)) {
          return `${path}.${key}: missing required property.`;
        }
      }

      if (!properties) {
        return null;
      }

      for (const [key, nestedSchema] of Object.entries(properties)) {
        if (!(key in value)) {
          continue;
        }

        const nestedError = validateSchema(value[key], nestedSchema as JsonValue, `${path}.${key}`);
        if (nestedError) {
          return nestedError;
        }
      }

      return null;
    }
    case 'array': {
      if (!Array.isArray(value)) {
        return `${path}: expected an array.`;
      }

      const itemSchema = schema.items as JsonValue | undefined;
      if (!itemSchema) {
        return null;
      }

      for (const [index, item] of value.entries()) {
        const nestedError = validateSchema(item, itemSchema, `${path}[${index}]`);
        if (nestedError) {
          return nestedError;
        }
      }

      return null;
    }
    case 'string': {
      if (typeof value !== 'string') {
        return `${path}: expected a string.`;
      }

      if (typeof schema.minLength === 'number' && value.length < schema.minLength) {
        return `${path}: expected a string with at least ${schema.minLength} characters.`;
      }

      return null;
    }
    case 'boolean':
      return typeof value === 'boolean' ? null : `${path}: expected a boolean.`;
    case 'number':
      return typeof value === 'number' && Number.isFinite(value)
        ? null
        : `${path}: expected a number.`;
    case 'integer':
      return typeof value === 'number' && Number.isInteger(value)
        ? null
        : `${path}: expected an integer.`;
    case 'null':
      return value === null ? null : `${path}: expected null.`;
    case undefined:
      return null;
    default:
      return `${path}: unsupported schema type '${type}'.`;
  }
}

function buildStructuredResult(
  text: string,
  responseMode: OpenClawConversationResponseMode,
): OpenClawConversationRunResult['response'] {
  if (responseMode.kind === 'text') {
    return {
      kind: 'text',
      text,
    };
  }

  const value = parseStructuredResponse(text);
  const validationError = validateSchema(value, responseMode.schema);
  if (validationError) {
    throw new Error(
      `OpenClaw structured response did not match the requested schema: ${validationError}`,
    );
  }

  return {
    kind: 'structured',
    text,
    value,
  };
}

function formatErrorMessage(error: unknown): string {
  if (error instanceof OpenClawConversationTimeoutError) {
    return error.message;
  }

  if (
    error instanceof Error &&
    (error.name === 'AbortError' || error.message.toLowerCase().includes('aborted'))
  ) {
    return 'OpenClaw conversation aborted before completion.';
  }

  return error instanceof Error ? error.message : String(error);
}

function coerceStoredRunResult(
  value: JsonValue | null,
): OpenClawConversationRunResult | null {
  if (!isJsonRecord(value) || value.action !== 'run') {
    return null;
  }

  return value as OpenClawConversationRunResult;
}

function normalizeSelectedModel(
  selectedModel: SelectedModel | null,
): OpenClawConversationRunResult['run']['selectedModel'] {
  if (!selectedModel) {
    return null;
  }

  return {
    provider: selectedModel.provider,
    model: selectedModel.model,
    ...(selectedModel.thinkingLevel
      ? { thinkingLevel: selectedModel.thinkingLevel }
      : {}),
  };
}

function createOpenClawSessionLogger(
  logger: RuntimeDependencies['logger'],
): OpenClawLogger {
  return {
    debug(message, ...context) {
      logger.debug(message, context as JsonValue[]);
    },
    info(message, ...context) {
      logger.info(message, context as JsonValue[]);
    },
    warn(message, ...context) {
      logger.warn(message, context as JsonValue[]);
    },
    error(message, ...context) {
      logger.error(message, context as JsonValue[]);
    },
  };
}

function resolveOpenClawSessionManager(
  deps: RuntimeDependencies,
  sessionManager?: OpenClawSessionManager,
): OpenClawSessionManager {
  if (sessionManager) {
    return sessionManager;
  }

  if (!sharedOpenClawSessionManager) {
    sharedOpenClawSessionManager = new SessionManager(
      {
        maxConcurrentSessions: Number.MAX_SAFE_INTEGER,
      },
      createOpenClawSessionLogger(deps.logger),
    );
  }

  return sharedOpenClawSessionManager;
}

function buildSessionStartConfig(
  sessionKey: string,
  workingDirectory: string,
  selection: ResolvedSessionSelection,
): OpenClawSessionStartConfig {
  return {
    name: sessionKey,
    cwd: workingDirectory,
    engine: selection.engine,
    model: selection.model,
  };
}

function isTimeoutError(error: unknown): boolean {
  const message = formatErrorMessage(error).toLowerCase();
  return message.includes('timeout');
}

function normalizeSessionError(error: unknown, timeoutMs: number): Error {
  if (error instanceof OpenClawConversationTimeoutError) {
    return error;
  }

  if (isTimeoutError(error)) {
    return new OpenClawConversationTimeoutError(timeoutMs);
  }

  return error instanceof Error ? error : new Error(String(error));
}

function hasActiveSession(
  sessionManager: OpenClawSessionManager,
  sessionKey: string,
): boolean {
  return sessionManager.listSessions().some((session) => session.name === sessionKey);
}

function hasPersistedSession(
  sessionManager: OpenClawSessionManager,
  sessionKey: string,
): boolean {
  return sessionManager
    .listPersistedSessions()
    .some((session) => session.name === sessionKey);
}

type CleanupOptions = {
  allowMissingSession: boolean;
};

async function cleanupSessionInternal(
  request: OpenClawConversationCleanupRequest,
  deps: RuntimeDependencies,
  sessionManager: OpenClawSessionManager,
  options: CleanupOptions,
): Promise<OpenClawConversationCleanupResult> {
  const sessionRecord = deps.stateStore.getOpenClawConversationSession(
    request.correlation.correlationKey,
    request.correlation.idempotencyKey,
  );

  if (!sessionRecord) {
    throw new Error(
      `OpenClaw conversation ${request.correlation.correlationKey}/${request.correlation.idempotencyKey} does not have Trigger-owned session state.`,
    );
  }

  const requestedSessionKey = sanitizeSessionKey(request.session?.key);
  if (requestedSessionKey && requestedSessionKey !== sessionRecord.sessionKey) {
    throw new Error(
      `OpenClaw conversation ${request.correlation.correlationKey}/${request.correlation.idempotencyKey} is bound to session ${sessionRecord.sessionKey}, not ${requestedSessionKey}.`,
    );
  }

  if (sessionRecord.status === 'cleaned') {
    return {
      action: 'cleanup',
      disposition: 'already-cleaned',
      session: {
        key: sessionRecord.sessionKey,
        cleanedUp: true,
      },
    };
  }

  const active = hasActiveSession(sessionManager, sessionRecord.sessionKey);
  const persisted = active || hasPersistedSession(sessionManager, sessionRecord.sessionKey);
  if (!persisted) {
    if (!options.allowMissingSession) {
      throw new Error(
        `OpenClaw session ${sessionRecord.sessionKey} is missing from standalone SessionManager persistence.`,
      );
    }
  } else {
    if (!active) {
      const selection = parseStandaloneSessionSelection(sessionRecord.engineModel);
      await sessionManager.startSession(
        buildSessionStartConfig(
          sessionRecord.sessionKey,
          sessionRecord.workingDirectory,
          selection,
        ),
      );
    }

    await sessionManager.stopSession(sessionRecord.sessionKey);
  }

  const cleanedSession = deps.stateStore.upsertOpenClawConversationSession({
    ...sessionRecord,
    status: 'cleaned',
  });

  return {
    action: 'cleanup',
    disposition: 'cleaned',
    session: {
      key: cleanedSession.sessionKey,
      cleanedUp: true,
    },
  };
}

function validateRunRequest(request: OpenClawConversationRunRequest): void {
  if (request.timeoutMs <= 0 || !Number.isFinite(request.timeoutMs)) {
    throw new Error('OpenClaw conversation timeoutMs must be a positive number.');
  }

  if (request.workingDirectory.trim().length === 0) {
    throw new Error('OpenClaw conversation workingDirectory must not be empty.');
  }
}

export async function runOpenClawConversationRequest(
  request: OpenClawConversationRunRequest,
  deps: RuntimeDependencies,
  sessionManager = resolveOpenClawSessionManager(deps),
): Promise<OpenClawConversationRunResult> {
  validateRunRequest(request);

  const claimKey = openClawConversationClaimKey(request.correlation.idempotencyKey);
  const claim = deps.stateStore.claim({
    claimKey,
    claimKind: 'event',
    workflowKey: OPENCLAW_CONVERSATION_WORKFLOW_KEY,
    scopeKey: request.correlation.correlationKey,
    metadata: {
      workflowKey: request.correlation.workflowKey,
      scopeKey: request.correlation.scopeKey,
    },
  });

  if (!claim.claimed) {
    if (claim.record.status === 'completed') {
      const storedResult = coerceStoredRunResult(claim.record.result);
      if (storedResult) {
        return storedResult;
      }

      throw new Error(
        `OpenClaw conversation run ${request.correlation.idempotencyKey} completed without a usable stored result.`,
      );
    }

    if (claim.record.status === 'failed') {
      throw new Error(
        claim.record.error ??
          `OpenClaw conversation run ${request.correlation.idempotencyKey} previously failed.`,
      );
    }

    throw new Error(
      `OpenClaw conversation run ${request.correlation.idempotencyKey} is already ${claim.record.status}.`,
    );
  }

  const startedAt = Date.now();
  const existingSessionRecord = deps.stateStore.getOpenClawConversationSession(
    request.correlation.correlationKey,
    request.correlation.idempotencyKey,
  );
  const profileName = resolveRequestedProfileName(request, deps);
  const sessionConfig = deps.environment.openClawSessionConfigResolver.resolve(
    profileName,
  );
  const engineModel = sessionConfig.engine.model;
  const selectedSession = parseStandaloneSessionSelection(engineModel);
  const sessionKey = resolveSessionKey(request, deps);

  let cleanupResult: OpenClawConversationCleanupResult | null = null;
  try {
    const startedSession = await sessionManager.startSession(
      buildSessionStartConfig(
        sessionKey,
        request.workingDirectory,
        selectedSession,
      ),
    );

    const activeSession = deps.stateStore.upsertOpenClawConversationSession({
      correlationKey: request.correlation.correlationKey,
      idempotencyKey: request.correlation.idempotencyKey,
      workflowKey: request.correlation.workflowKey,
      scopeKey: request.correlation.scopeKey,
      sessionKey,
      sessionId:
        startedSession.claudeSessionId ?? existingSessionRecord?.sessionId ?? null,
      profileName,
      engineModel,
      workingDirectory: request.workingDirectory,
      status: 'active',
    });

    deps.stateStore.markClaimDispatched(claimKey, {
      sessionKey,
      profileName,
      engineModel,
    });

    const prompt = buildPrompt(request);
    let response: SendResult;
    try {
      response = await sessionManager.sendMessage(sessionKey, prompt, {
        timeout: request.timeoutMs,
      });
    } catch (error) {
      throw normalizeSessionError(error, request.timeoutMs);
    }

    if (response.error) {
      throw new Error(response.error);
    }

    const text = response.output.trim();
    if (text.length === 0) {
      throw new Error('OpenClaw conversation completed without a text response.');
    }

    const persistedSession = deps.stateStore.upsertOpenClawConversationSession({
      ...activeSession,
      sessionId: response.sessionId ?? activeSession.sessionId,
      status: 'active',
    });

    const result: OpenClawConversationRunResult = {
      action: 'run',
      session: {
        key: persistedSession.sessionKey,
        cleanedUp: false,
      },
      selection: {
        profileName,
        engineModel,
        modelRef: selectedSession.modelRef,
      },
      response: buildStructuredResult(text, request.response),
      run: {
        claimKey,
        idempotencyKey: request.correlation.idempotencyKey,
        durationMs: Date.now() - startedAt,
        agentRunId: null,
        selectedModel: normalizeSelectedModel({
          provider: selectedSession.provider,
          model: selectedSession.model,
        }),
      },
    };

    if (request.cleanup.onSuccess === 'delete') {
      cleanupResult = await cleanupSessionInternal(
        {
          action: 'cleanup',
          correlation: request.correlation,
          session: {
            key: persistedSession.sessionKey,
          },
        },
        deps,
        sessionManager,
        { allowMissingSession: false },
      );
      result.session.cleanedUp = cleanupResult.session.cleanedUp;
    }

    deps.stateStore.markClaimCompleted(claimKey, result);
    return result;
  } catch (error) {
    let thrownError = error;

    if (request.cleanup.onError === 'delete') {
      try {
        cleanupResult = await cleanupSessionInternal(
          {
            action: 'cleanup',
            correlation: request.correlation,
            session: { key: sessionKey },
          },
          deps,
          sessionManager,
          { allowMissingSession: true },
        );
      } catch (cleanupError) {
        const originalMessage = formatErrorMessage(error);
        const cleanupMessage = formatErrorMessage(cleanupError);
        thrownError = new Error(
          `${originalMessage} Cleanup also failed: ${cleanupMessage}`,
        );
      }
    }

    const errorMessage = formatErrorMessage(thrownError);
    deps.stateStore.markClaimFailed(claimKey, errorMessage);

    if (cleanupResult) {
      deps.logger.warn('OpenClaw conversation failed after cleanup', {
        correlationKey: request.correlation.correlationKey,
        idempotencyKey: request.correlation.idempotencyKey,
        sessionKey: cleanupResult.session.key,
        error: errorMessage,
      });
    }

    throw thrownError;
  }
}

export async function cleanupOpenClawConversationRequest(
  request: OpenClawConversationCleanupRequest,
  deps: RuntimeDependencies,
  sessionManager = resolveOpenClawSessionManager(deps),
): Promise<OpenClawConversationCleanupResult> {
  return cleanupSessionInternal(request, deps, sessionManager, {
    allowMissingSession: false,
  });
}

export async function executeOpenClawConversationRequest(
  request: OpenClawConversationRequest,
  deps: RuntimeDependencies,
  sessionManager = resolveOpenClawSessionManager(deps),
): Promise<OpenClawConversationResult> {
  if (request.action === 'cleanup') {
    return cleanupOpenClawConversationRequest(request, deps, sessionManager);
  }

  return runOpenClawConversationRequest(request, deps, sessionManager);
}

export {
  OPENCLAW_CONVERSATION_WORKFLOW_KEY,
  openClawConversationClaimKey,
};
