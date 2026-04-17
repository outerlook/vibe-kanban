import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

import {
  applyModelOverrideToSessionEntry,
  loadSessionStore,
  resolveSessionStoreEntry,
  resolveStorePath,
  saveSessionStore,
  type OpenClawConfig,
} from 'openclaw/plugin-sdk/config-runtime';
import {
  getReplyFromConfig,
  type MsgContext,
  type ReplyPayload,
} from 'openclaw/plugin-sdk/reply-runtime';

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
  thinkLevel?: string;
};

export type OpenClawSdk = {
  applyModelOverrideToSessionEntry: typeof applyModelOverrideToSessionEntry;
  getReplyFromConfig: typeof getReplyFromConfig;
  loadSessionStore: typeof loadSessionStore;
  resolveSessionStoreEntry: typeof resolveSessionStoreEntry;
  resolveStorePath: typeof resolveStorePath;
  saveSessionStore: typeof saveSessionStore;
};

const defaultOpenClawSdk: OpenClawSdk = {
  applyModelOverrideToSessionEntry,
  getReplyFromConfig,
  loadSessionStore,
  resolveSessionStoreEntry,
  resolveStorePath,
  saveSessionStore,
};

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

function deriveSessionKey(correlationKey: string): string {
  return `trigger:openclaw:${encodeSessionKeyPart(correlationKey)}`;
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

function toOpenClawModelRef(engineModel: string): string {
  const trimmed = engineModel.trim();
  if (trimmed.length === 0) {
    throw new Error('Resolved OpenClaw engine.model must not be empty.');
  }

  if (trimmed.includes('/')) {
    return trimmed;
  }

  const firstDotIndex = trimmed.indexOf('.');
  if (firstDotIndex <= 0 || firstDotIndex === trimmed.length - 1) {
    throw new Error(
      `Resolved OpenClaw engine.model '${trimmed}' must use <ENGINE>.<MODEL> or <ENGINE>/<MODEL>.`,
    );
  }

  return `${trimmed.slice(0, firstDotIndex)}/${trimmed.slice(firstDotIndex + 1)}`;
}

function splitModelRef(modelRef: string): { provider: string; model: string } {
  const separatorIndex = modelRef.indexOf('/');
  if (separatorIndex <= 0 || separatorIndex === modelRef.length - 1) {
    throw new Error(
      `OpenClaw model reference '${modelRef}' must use <PROVIDER>/<MODEL>.`,
    );
  }

  return {
    provider: modelRef.slice(0, separatorIndex),
    model: modelRef.slice(separatorIndex + 1),
  };
}

function resolveSessionStorePath(
  deps: RuntimeDependencies,
  sdk: OpenClawSdk,
): string {
  const defaultStorePath = join(
    dirname(deps.environment.stateDatabasePath),
    'openclaw-sessions.json',
  );
  const resolvedStorePath = sdk.resolveStorePath(defaultStorePath);
  mkdirSync(dirname(resolvedStorePath), { recursive: true });
  return resolvedStorePath;
}

function buildConfigOverride(
  request: OpenClawConversationRunRequest,
  modelRef: string,
  storePath: string,
): OpenClawConfig {
  return {
    session: {
      store: storePath,
    },
    agents: {
      defaults: {
        model: modelRef,
        workspace: request.workingDirectory,
      },
    },
  } satisfies OpenClawConfig;
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

function buildMessageContext(
  request: OpenClawConversationRunRequest,
  sessionKey: string,
  prompt: string,
): MsgContext {
  const from = `trigger:${encodeSessionKeyPart(request.correlation.workflowKey)}`;
  return {
    Provider: 'trigger',
    Surface: 'trigger',
    ChatType: 'direct',
    Body: prompt,
    BodyForAgent: prompt,
    BodyForCommands: prompt,
    RawBody: prompt,
    CommandBody: prompt,
    SessionKey: sessionKey,
    From: from,
    To: 'trigger:openclaw',
    Timestamp: Date.now(),
  } satisfies MsgContext;
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
  );
  const explicitSessionKey = sanitizeSessionKey(request.session?.key);

  if (existing?.status === 'active') {
    if (explicitSessionKey && explicitSessionKey !== existing.sessionKey) {
      throw new Error(
        `OpenClaw conversation ${request.correlation.correlationKey} is already bound to session ${existing.sessionKey}.`,
      );
    }

    return existing.sessionKey;
  }

  return explicitSessionKey ?? deriveSessionKey(request.correlation.correlationKey);
}

function toPayloadList(
  response: ReplyPayload | ReplyPayload[] | undefined,
  collectedPayloads: Array<ReplyPayload | ReplyPayload[]>,
): ReplyPayload[] {
  if (Array.isArray(response) && response.length > 0) {
    return response;
  }

  if (!Array.isArray(response) && response) {
    return [response];
  }

  return collectedPayloads.flatMap((payload) =>
    Array.isArray(payload) ? payload : [payload],
  );
}

function extractResponseText(payloads: ReplyPayload[]): string {
  const text = payloads
    .filter((payload) => payload.isReasoning !== true)
    .filter((payload) => payload.isCompactionNotice !== true)
    .map((payload) => payload.text?.trim())
    .filter((value): value is string => Boolean(value))
    .join('\n\n')
    .trim();

  if (text.length === 0) {
    throw new Error('OpenClaw conversation completed without a text response.');
  }

  return text;
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

      if (schema.additionalProperties === false && properties) {
        for (const key of Object.keys(value)) {
          if (!(key in properties)) {
            return `${path}.${key}: additional properties are not allowed.`;
          }
        }
      }

      if (!properties) {
        return null;
      }

      for (const [key, propertySchema] of Object.entries(properties)) {
        if (!(key in value)) {
          continue;
        }

        const nestedError = validateSchema(value[key], propertySchema as JsonValue, `${path}.${key}`);
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
      if (itemSchema === undefined) {
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
    ...(selectedModel.thinkLevel
      ? { thinkingLevel: selectedModel.thinkLevel }
      : {}),
  };
}

type CleanupOptions = {
  allowMissingSession: boolean;
};

async function cleanupSessionInternal(
  request: OpenClawConversationCleanupRequest,
  deps: RuntimeDependencies,
  sdk: OpenClawSdk,
  options: CleanupOptions,
): Promise<OpenClawConversationCleanupResult> {
  const sessionRecord = deps.stateStore.getOpenClawConversationSession(
    request.correlation.correlationKey,
  );

  if (!sessionRecord) {
    throw new Error(
      `OpenClaw conversation ${request.correlation.correlationKey} does not have Trigger-owned session state.`,
    );
  }

  const requestedSessionKey = sanitizeSessionKey(request.session?.key);
  if (requestedSessionKey && requestedSessionKey !== sessionRecord.sessionKey) {
    throw new Error(
      `OpenClaw conversation ${request.correlation.correlationKey} is bound to session ${sessionRecord.sessionKey}, not ${requestedSessionKey}.`,
    );
  }

  if (sessionRecord.status === 'cleaned') {
    return {
      action: 'cleanup',
      disposition: 'already-cleaned',
      session: {
        key: sessionRecord.sessionKey,
        id: sessionRecord.sessionId,
        storePath: sessionRecord.sessionStorePath,
        cleanedUp: true,
      },
    };
  }

  const store = sdk.loadSessionStore(sessionRecord.sessionStorePath);
  const resolvedSession = sdk.resolveSessionStoreEntry({
    store,
    sessionKey: sessionRecord.sessionKey,
  });

  const sessionEntry = resolvedSession.existing;
  if (!sessionEntry) {
    if (!options.allowMissingSession && sessionRecord.sessionId) {
      throw new Error(
        `OpenClaw session ${sessionRecord.sessionKey} is missing from ${sessionRecord.sessionStorePath}.`,
      );
    }
  } else {
    delete store[resolvedSession.normalizedKey];
    await sdk.saveSessionStore(sessionRecord.sessionStorePath, store);

    const sessionFile = typeof sessionEntry.sessionFile === 'string'
      ? sessionEntry.sessionFile.trim()
      : '';
    if (sessionFile.length > 0) {
      const storeDirectory = resolve(dirname(sessionRecord.sessionStorePath));
      const resolvedSessionFile = resolve(
        isAbsolute(sessionFile)
          ? sessionFile
          : join(storeDirectory, sessionFile),
      );
      const relativePath = relative(storeDirectory, resolvedSessionFile);

      // Keep cleanup constrained to OpenClaw's session store directory so
      // persisted transcript metadata cannot target arbitrary filesystem paths.
      if (
        relativePath === '' ||
        (relativePath !== '..' && !relativePath.startsWith(`..${sep}`))
      ) {
        if (existsSync(resolvedSessionFile)) {
          rmSync(resolvedSessionFile);
        }
      } else {
        throw new Error(
          `Refusing to delete OpenClaw transcript outside the session store directory: ${resolvedSessionFile}`,
        );
      }
    }
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
      id: cleanedSession.sessionId,
      storePath: cleanedSession.sessionStorePath,
      cleanedUp: true,
    },
  };
}

function createAbortController(timeoutMs: number): {
  controller: AbortController;
  clear: () => void;
} {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  return {
    controller,
    clear() {
      clearTimeout(timeout);
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
  sdk: OpenClawSdk = defaultOpenClawSdk,
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
  );
  const profileName = resolveRequestedProfileName(request, deps);
  const sessionConfig = deps.environment.openClawSessionConfigResolver.resolve(
    profileName,
  );
  const engineModel = sessionConfig.engine.model;
  const modelRef = toOpenClawModelRef(engineModel);
  const { provider, model } = splitModelRef(modelRef);
  const sessionStorePath = resolveSessionStorePath(deps, sdk);
  const sessionKey = resolveSessionKey(request, deps);

  const activeSession = deps.stateStore.upsertOpenClawConversationSession({
    correlationKey: request.correlation.correlationKey,
    workflowKey: request.correlation.workflowKey,
    scopeKey: request.correlation.scopeKey,
    sessionKey,
    sessionId: existingSessionRecord?.sessionId ?? null,
    sessionStorePath,
    profileName,
    engineModel,
    workingDirectory: request.workingDirectory,
    status: 'active',
  });

  const store = sdk.loadSessionStore(sessionStorePath);
  const resolvedExistingSession = sdk.resolveSessionStoreEntry({
    store,
    sessionKey,
  });
  if (resolvedExistingSession.existing) {
    const overrideResult = sdk.applyModelOverrideToSessionEntry({
      entry: resolvedExistingSession.existing,
      selection: {
        provider,
        model,
      },
      profileOverride: profileName,
      profileOverrideSource: 'auto',
      selectionSource: 'auto',
    });

    if (overrideResult.updated) {
      store[resolvedExistingSession.normalizedKey] = resolvedExistingSession.existing;
      await sdk.saveSessionStore(sessionStorePath, store);
    }
  }

  deps.stateStore.markClaimDispatched(claimKey, {
    sessionKey,
    profileName,
    engineModel,
  });

  let cleanupResult: OpenClawConversationCleanupResult | null = null;
  try {
    const prompt = buildPrompt(request);
    const context = buildMessageContext(request, sessionKey, prompt);
    const configOverride = buildConfigOverride(request, modelRef, sessionStorePath);
    const observedPayloads: Array<ReplyPayload | ReplyPayload[]> = [];
    let selectedModel: SelectedModel | null = null;
    let agentRunId: string | null = null;

    const { controller, clear } = createAbortController(request.timeoutMs);
    let response: ReplyPayload | ReplyPayload[] | undefined;
    try {
      response = await sdk.getReplyFromConfig(
        context,
        {
          abortSignal: controller.signal,
          onAgentRunStart(runId) {
            agentRunId = runId;
          },
          onBlockReply(payload) {
            observedPayloads.push(payload);
          },
          onModelSelected(modelSelection) {
            selectedModel = modelSelection as SelectedModel;
          },
        },
        configOverride,
      );
    } catch (error) {
      if (controller.signal.aborted) {
        throw new OpenClawConversationTimeoutError(request.timeoutMs);
      }

      throw error;
    } finally {
      clear();
    }

    const payloads = toPayloadList(response, observedPayloads);
    const text = extractResponseText(payloads);
    const refreshedStore = sdk.loadSessionStore(sessionStorePath);
    const resolvedSession = sdk.resolveSessionStoreEntry({
      store: refreshedStore,
      sessionKey,
    });
    if (!resolvedSession.existing) {
      throw new Error(
        `OpenClaw session ${sessionKey} was not persisted to ${sessionStorePath}.`,
      );
    }

    const persistedSession = deps.stateStore.upsertOpenClawConversationSession({
      ...activeSession,
      sessionId: resolvedSession.existing.sessionId ?? null,
      status: 'active',
    });

    const result: OpenClawConversationRunResult = {
      action: 'run',
      session: {
        key: persistedSession.sessionKey,
        id: persistedSession.sessionId,
        storePath: persistedSession.sessionStorePath,
        cleanedUp: false,
      },
      selection: {
        profileName,
        engineModel,
        modelRef,
      },
      response: buildStructuredResult(text, request.response),
      run: {
        claimKey,
        idempotencyKey: request.correlation.idempotencyKey,
        durationMs: Date.now() - startedAt,
        agentRunId,
        selectedModel: normalizeSelectedModel(selectedModel),
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
        sdk,
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
          sdk,
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
  sdk: OpenClawSdk = defaultOpenClawSdk,
): Promise<OpenClawConversationCleanupResult> {
  return cleanupSessionInternal(request, deps, sdk, {
    allowMissingSession: false,
  });
}

export async function executeOpenClawConversationRequest(
  request: OpenClawConversationRequest,
  deps: RuntimeDependencies,
  sdk: OpenClawSdk = defaultOpenClawSdk,
): Promise<OpenClawConversationResult> {
  if (request.action === 'cleanup') {
    return cleanupOpenClawConversationRequest(request, deps, sdk);
  }

  return runOpenClawConversationRequest(request, deps, sdk);
}

export {
  OPENCLAW_CONVERSATION_WORKFLOW_KEY,
  openClawConversationClaimKey,
};
