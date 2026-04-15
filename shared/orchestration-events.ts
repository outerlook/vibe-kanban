import type {
  FollowUpScope,
  OrchestrationEventEnvelope,
  OrchestrationEventType,
} from './types';
import { DEFAULT_ORCHESTRATION_EVENT_SCHEMA_VERSION } from './types';

export const ORCHESTRATION_SCHEMA_VERSION =
  DEFAULT_ORCHESTRATION_EVENT_SCHEMA_VERSION;

export const ORCHESTRATION_EVENT_TYPES = [
  'task_created',
  'task_updated',
  'task_deleted',
  'task_status_changed',
  'execution_started',
  'execution_completed',
  'workspace_created',
  'workspace_deleted',
  'project_updated',
  'approval_requested',
  'approval_resolved',
  'conversation_message_added',
  'follow_up_transition',
  'merge_queue_transition',
  'task_group_transition',
  'task_group_completed',
] as const satisfies ReadonlyArray<OrchestrationEventType>;

export type OrchestrationTriggerRefs = {
  taskContext: { taskId: string; projectId: string | null } | null;
  taskGroupContext: { taskGroupId: string } | null;
  conversationContext: { conversationId: string } | null;
  executionContext: { executionProcessId: string } | null;
  approvalContext: { approvalId: string } | null;
  taskSession: { sessionId: string } | null;
};

export type OrchestrationTriggerItem = {
  topic: string;
  eventId: string;
  schemaVersion: string;
  occurredAt: string;
  eventType: OrchestrationEventType;
  entityIds: {
    taskId: string | null;
    workspaceId: string | null;
    sessionId: string | null;
    executionProcessId: string | null;
    taskGroupId: string | null;
  };
  payload: OrchestrationEventEnvelope['payload'];
  refs: OrchestrationTriggerRefs;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function asOptionalString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readStringField(record: Record<string, unknown>, field: string): string {
  const value = record[field];
  if (typeof value !== 'string') {
    throw new Error(`Orchestration event is missing ${field}`);
  }

  return value;
}

function readNullableStringField(
  record: Record<string, unknown>,
  field: string,
): string | null {
  const value = record[field];
  if (value === null) {
    return null;
  }

  if (typeof value !== 'string') {
    throw new Error(`Orchestration event has invalid ${field}`);
  }

  return value;
}

function readEventType(record: Record<string, unknown>): OrchestrationEventType {
  const eventType = readStringField(record, 'event_type');
  if (!ORCHESTRATION_EVENT_TYPES.includes(eventType as OrchestrationEventType)) {
    throw new Error(`Unsupported orchestration event type '${eventType}'`);
  }

  return eventType as OrchestrationEventType;
}

function topicSuffix(topic: string): string {
  return topic.split('/').filter(Boolean).at(-1) ?? '';
}

export function topicForOrchestrationEvent(
  topicNamespace: string,
  eventType: OrchestrationEventType,
): string {
  return `${topicNamespace.replace(/\/+$/, '')}/${eventType}`;
}

export function parseOrchestrationEvent(
  raw: Buffer | string,
  topic: string,
  expectedSchemaVersion = ORCHESTRATION_SCHEMA_VERSION,
): OrchestrationTriggerItem {
  const parsed = JSON.parse(raw.toString()) as OrchestrationEventEnvelope;

  if (!isRecord(parsed)) {
    throw new Error('Orchestration event must be a JSON object');
  }

  const schemaVersion = readStringField(parsed, 'schema_version');
  const eventId = readStringField(parsed, 'event_id');
  const occurredAt = readStringField(parsed, 'occurred_at');
  const eventType = readEventType(parsed);

  if (schemaVersion !== expectedSchemaVersion) {
    throw new Error(
      `Unsupported orchestration schema '${schemaVersion}', expected '${expectedSchemaVersion}'`,
    );
  }

  if (topicSuffix(topic) !== eventType) {
    throw new Error(
      `Orchestration topic '${topic}' does not match event type '${eventType}'`,
    );
  }

  if (!isRecord(parsed.payload)) {
    throw new Error('Orchestration event payload must be a JSON object');
  }

  const payload = parsed.payload as Record<string, unknown>;
  const taskId = readNullableStringField(parsed, 'task_id');
  const workspaceId = readNullableStringField(parsed, 'workspace_id');
  const sessionId = readNullableStringField(parsed, 'session_id');
  const executionProcessId = readNullableStringField(parsed, 'execution_process_id');
  const taskGroupId = readNullableStringField(parsed, 'task_group_id');
  const followUpScope = asOptionalString(payload.scope) as FollowUpScope | null;
  const payloadExecutionProcessId = asOptionalString(payload.execution_process_id);
  const conversationId =
    asOptionalString(payload.conversation_session_id) ??
    (followUpScope === 'conversation' ? sessionId : null);

  return {
    topic,
    eventId,
    schemaVersion,
    occurredAt,
    eventType,
    entityIds: {
      taskId,
      workspaceId,
      sessionId,
      executionProcessId: executionProcessId ?? payloadExecutionProcessId,
      taskGroupId,
    },
    payload: parsed.payload,
    refs: {
      taskContext: taskId
        ? {
            taskId,
            projectId: asOptionalString(payload.project_id),
          }
        : null,
      taskGroupContext: taskGroupId ? { taskGroupId } : null,
      conversationContext: conversationId ? { conversationId } : null,
      executionContext: executionProcessId ?? payloadExecutionProcessId
        ? {
            executionProcessId:
              executionProcessId ?? payloadExecutionProcessId ?? '',
          }
        : null,
      approvalContext: asOptionalString(payload.approval_id)
        ? { approvalId: String(payload.approval_id) }
        : null,
      taskSession:
        followUpScope === 'task_session' && sessionId ? { sessionId } : null,
    },
  };
}
