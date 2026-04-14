"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VibeKanbanTrigger = void 0;
const n8n_workflow_1 = require("n8n-workflow");
const constants_1 = require("./shared/constants");
const events_1 = require("./shared/events");
class VibeKanbanTrigger {
    description = {
        displayName: 'Vibe Kanban Trigger',
        name: 'vibeKanbanTrigger',
        icon: 'file:vibeKanban.svg',
        group: ['trigger'],
        version: 1,
        subtitle: '={{$parameter["eventTypes"].join(", ")}}',
        description: 'Listen to first-class VK orchestration events over MQTT',
        defaults: {
            name: 'Vibe Kanban Trigger',
        },
        triggerPanel: {
            header: 'Listen for VK orchestration lifecycle events',
            executionsHelp: {
                inactive: 'Execute the node, then publish a VK orchestration event to preview the emitted payload.',
                active: 'This workflow stays subscribed to VK orchestration topics and runs whenever matching events arrive.',
            },
            activationHint: 'Activate the workflow once the downstream VK read/action steps are ready.',
        },
        inputs: [],
        outputs: [n8n_workflow_1.NodeConnectionTypes.Main],
        credentials: [
            {
                name: 'vibeKanbanMqtt',
                required: true,
            },
        ],
        properties: [
            {
                displayName: 'Event Types',
                name: 'eventTypes',
                type: 'multiOptions',
                default: [...constants_1.VK_EVENT_TYPES],
                required: true,
                options: constants_1.VK_EVENT_TYPES.map((value) => ({
                    name: value,
                    value,
                })),
                description: 'VK orchestration topics this trigger subscribes to',
            },
            {
                displayName: 'Schema Version',
                name: 'schemaVersion',
                type: 'string',
                default: constants_1.VK_ORCHESTRATION_SCHEMA_VERSION,
                required: true,
                description: 'Reject MQTT events that do not match this VK event schema version',
            },
        ],
    };
    async trigger() {
        const eventTypes = this.getNodeParameter('eventTypes');
        if (eventTypes.length === 0) {
            throw new n8n_workflow_1.NodeOperationError(this.getNode(), 'Select at least one VK event type');
        }
        const schemaVersion = this.getNodeParameter('schemaVersion');
        const credentials = await this.getCredentials('vibeKanbanMqtt');
        const client = await (0, events_1.createVkMqttClient)(credentials);
        const emitEvent = (event) => {
            this.emit([
                this.helpers.returnJsonArray([
                    JSON.parse(JSON.stringify(event)),
                ]),
            ]);
        };
        const manualTriggerFunction = async () => {
            await new Promise((resolve, reject) => {
                const topics = eventTypes.map((eventType) => `${credentials.topicNamespace.replace(/\/+$/, '')}/${eventType}`);
                const handleManualMessage = (topic, payload) => {
                    try {
                        emitEvent((0, events_1.parseVkOrchestrationEvent)(payload, topic, schemaVersion));
                        client.off('message', handleManualMessage);
                        resolve();
                    }
                    catch (error) {
                        client.off('message', handleManualMessage);
                        reject(error);
                    }
                };
                client.on('message', handleManualMessage);
                client.subscribe(topics, (error) => {
                    if (error) {
                        client.off('message', handleManualMessage);
                        reject(error);
                    }
                });
            });
        };
        if (this.getMode() === 'trigger') {
            await (0, events_1.subscribeToVkEvents)({
                client,
                eventTypes,
                topicNamespace: credentials.topicNamespace,
                schemaVersion,
                onEvent: emitEvent,
                onError: (error) => {
                    console.error('Failed to parse VK MQTT event', error);
                },
            });
        }
        return {
            closeFunction: async () => await (0, events_1.closeVkMqttClient)(client),
            manualTriggerFunction,
        };
    }
}
exports.VibeKanbanTrigger = VibeKanbanTrigger;
//# sourceMappingURL=VibeKanbanTrigger.node.js.map