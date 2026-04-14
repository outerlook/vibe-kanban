import { connect, type MqttClient } from 'mqtt';

import { VK_ORCHESTRATION_SCHEMA_VERSION, type VkEventType } from './constants';
import type {
  VkMqttCredentialValue,
  VkOrchestrationEventEnvelope,
} from './vk-contracts';

export type VkTriggerRefs = {
  taskContext: { taskId: string; projectId: string | null } | null;
  taskGroupContext: { taskGroupId: string } | null;
  conversationContext: { conversationId: string } | null;
  executionContext: { executionProcessId: string } | null;
  approvalContext: { approvalId: string } | null;
  taskSession: { sessionId: string } | null;
};

export type VkTriggerItem = {
  topic: string;
  eventId: string;
  schemaVersion: string;
  occurredAt: string;
  eventType: string;
  entityIds: {
    taskId: string | null;
    workspaceId: string | null;
    sessionId: string | null;
    executionProcessId: string | null;
    taskGroupId: string | null;
  };
  payload: unknown;
  refs: VkTriggerRefs;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function asOptionalString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

export function topicForEvent(topicNamespace: string, eventType: VkEventType): string {
  return `${topicNamespace.replace(/\/+$/, '')}/${eventType}`;
}

export function parseVkOrchestrationEvent(
  raw: Buffer | string,
  topic: string,
  expectedSchemaVersion = VK_ORCHESTRATION_SCHEMA_VERSION,
): VkTriggerItem {
  const parsed = JSON.parse(raw.toString()) as VkOrchestrationEventEnvelope;

  if (!isRecord(parsed)) {
    throw new Error('VK orchestration event must be a JSON object');
  }

  if (typeof parsed.schema_version !== 'string') {
    throw new Error('VK orchestration event is missing schema_version');
  }

  if (parsed.schema_version !== expectedSchemaVersion) {
    throw new Error(
      `Unsupported VK orchestration schema '${parsed.schema_version}', expected '${expectedSchemaVersion}'`,
    );
  }

  if (typeof parsed.event_type !== 'string') {
    throw new Error('VK orchestration event is missing event_type');
  }

  const payload = isRecord(parsed.payload)
    ? (parsed.payload as Record<string, unknown>)
    : {};
  const sessionId = asOptionalString(parsed.session_id);
  const followUpScope = asOptionalString(payload.scope);
  const conversationId =
    asOptionalString(payload.conversation_session_id) ??
    (followUpScope === 'conversation' ? sessionId : null);

  return {
    topic,
    eventId: String(parsed.event_id),
    schemaVersion: parsed.schema_version,
    occurredAt: String(parsed.occurred_at),
    eventType: parsed.event_type,
    entityIds: {
      taskId: asOptionalString(parsed.task_id),
      workspaceId: asOptionalString(parsed.workspace_id),
      sessionId,
      executionProcessId:
        asOptionalString(parsed.execution_process_id) ??
        asOptionalString(payload.execution_process_id),
      taskGroupId: asOptionalString(parsed.task_group_id),
    },
    payload: parsed.payload,
    refs: {
      taskContext: parsed.task_id
        ? {
            taskId: String(parsed.task_id),
            projectId: asOptionalString(payload.project_id),
          }
        : null,
      taskGroupContext: parsed.task_group_id
        ? { taskGroupId: String(parsed.task_group_id) }
        : null,
      conversationContext: conversationId ? { conversationId } : null,
      executionContext:
        asOptionalString(parsed.execution_process_id) ??
        asOptionalString(payload.execution_process_id)
          ? {
              executionProcessId: String(
                asOptionalString(parsed.execution_process_id) ??
                  asOptionalString(payload.execution_process_id),
              ),
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

export async function createVkMqttClient(
  credentials: VkMqttCredentialValue,
): Promise<MqttClient> {
  return await new Promise<MqttClient>((resolve, reject) => {
    const client = connect(credentials.brokerUrl, {
      username: credentials.username || undefined,
      password: credentials.password || undefined,
      clientId: credentials.clientId || undefined,
      clean: credentials.clean ?? true,
    });

    const onConnect = () => {
      client.off('error', onError);
      resolve(client);
    };

    const onError = (error: Error) => {
      client.off('connect', onConnect);
      client.end(true);
      reject(error);
    };

    client.once('connect', onConnect);
    client.once('error', onError);
  });
}

export async function subscribeToVkEvents(args: {
  client: MqttClient;
  eventTypes: VkEventType[];
  topicNamespace: string;
  schemaVersion?: string;
  onEvent: (event: VkTriggerItem) => void;
  onError?: (error: Error) => void;
}): Promise<void> {
  const topics = args.eventTypes.map((eventType) =>
    topicForEvent(args.topicNamespace, eventType),
  );

  await new Promise<void>((resolve, reject) => {
    args.client.subscribe(topics, { qos: 0 }, (error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });

  args.client.on('message', (topic, payload) => {
    try {
      args.onEvent(
        parseVkOrchestrationEvent(
          payload,
          topic,
          args.schemaVersion ?? VK_ORCHESTRATION_SCHEMA_VERSION,
        ),
      );
    } catch (error) {
      args.onError?.(
        error instanceof Error ? error : new Error('Failed to parse VK event'),
      );
    }
  });
}

export async function closeVkMqttClient(client: MqttClient): Promise<void> {
  await new Promise<void>((resolve) => {
    client.end(true, {}, () => resolve());
  });
}
