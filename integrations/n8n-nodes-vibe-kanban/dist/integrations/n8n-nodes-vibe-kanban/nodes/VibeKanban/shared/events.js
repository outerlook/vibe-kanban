"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.topicForEvent = topicForEvent;
exports.parseVkOrchestrationEvent = parseVkOrchestrationEvent;
exports.createVkMqttClient = createVkMqttClient;
exports.subscribeToVkEvents = subscribeToVkEvents;
exports.closeVkMqttClient = closeVkMqttClient;
const mqtt_1 = require("mqtt");
const constants_1 = require("./constants");
function isRecord(value) {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function asOptionalString(value) {
    return typeof value === 'string' ? value : null;
}
function topicForEvent(topicNamespace, eventType) {
    return `${topicNamespace.replace(/\/+$/, '')}/${eventType}`;
}
function parseVkOrchestrationEvent(raw, topic, expectedSchemaVersion = constants_1.VK_ORCHESTRATION_SCHEMA_VERSION) {
    const parsed = JSON.parse(raw.toString());
    if (!isRecord(parsed)) {
        throw new Error('VK orchestration event must be a JSON object');
    }
    if (typeof parsed.schema_version !== 'string') {
        throw new Error('VK orchestration event is missing schema_version');
    }
    if (parsed.schema_version !== expectedSchemaVersion) {
        throw new Error(`Unsupported VK orchestration schema '${parsed.schema_version}', expected '${expectedSchemaVersion}'`);
    }
    if (typeof parsed.event_type !== 'string') {
        throw new Error('VK orchestration event is missing event_type');
    }
    const payload = isRecord(parsed.payload)
        ? parsed.payload
        : {};
    const sessionId = asOptionalString(parsed.session_id);
    const followUpScope = asOptionalString(payload.scope);
    const conversationId = asOptionalString(payload.conversation_session_id) ??
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
            executionProcessId: asOptionalString(parsed.execution_process_id) ??
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
            executionContext: asOptionalString(parsed.execution_process_id) ??
                asOptionalString(payload.execution_process_id)
                ? {
                    executionProcessId: String(asOptionalString(parsed.execution_process_id) ??
                        asOptionalString(payload.execution_process_id)),
                }
                : null,
            approvalContext: asOptionalString(payload.approval_id)
                ? { approvalId: String(payload.approval_id) }
                : null,
            taskSession: followUpScope === 'task_session' && sessionId ? { sessionId } : null,
        },
    };
}
async function createVkMqttClient(credentials) {
    return await new Promise((resolve, reject) => {
        const client = (0, mqtt_1.connect)(credentials.brokerUrl, {
            username: credentials.username || undefined,
            password: credentials.password || undefined,
            clientId: credentials.clientId || undefined,
            clean: credentials.clean ?? true,
        });
        const onConnect = () => {
            client.off('error', onError);
            resolve(client);
        };
        const onError = (error) => {
            client.off('connect', onConnect);
            client.end(true);
            reject(error);
        };
        client.once('connect', onConnect);
        client.once('error', onError);
    });
}
async function subscribeToVkEvents(args) {
    const topics = args.eventTypes.map((eventType) => topicForEvent(args.topicNamespace, eventType));
    await new Promise((resolve, reject) => {
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
            args.onEvent(parseVkOrchestrationEvent(payload, topic, args.schemaVersion ?? constants_1.VK_ORCHESTRATION_SCHEMA_VERSION));
        }
        catch (error) {
            args.onError?.(error instanceof Error ? error : new Error('Failed to parse VK event'));
        }
    });
}
async function closeVkMqttClient(client) {
    await new Promise((resolve) => {
        client.end(true, {}, () => resolve());
    });
}
//# sourceMappingURL=events.js.map