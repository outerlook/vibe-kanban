import type {
  IDataObject,
  ITriggerFunctions,
  INodeType,
  INodeTypeDescription,
  ITriggerResponse,
} from 'n8n-workflow';
import { NodeConnectionTypes, NodeOperationError } from 'n8n-workflow';

import {
  VK_EVENT_TYPES,
  VK_ORCHESTRATION_SCHEMA_VERSION,
  type VkEventType,
} from './shared/constants';
import {
  closeVkMqttClient,
  createVkMqttClient,
  parseVkOrchestrationEvent,
  subscribeToVkEvents,
} from './shared/events';
import type { VkMqttCredentialValue } from './shared/vk-contracts';

export class VibeKanbanTrigger implements INodeType {
  description: INodeTypeDescription = {
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
        inactive:
          'Execute the node, then publish a VK orchestration event to preview the emitted payload.',
        active:
          'This workflow stays subscribed to VK orchestration topics and runs whenever matching events arrive.',
      },
      activationHint:
        'Activate the workflow once the downstream VK read/action steps are ready.',
    },
    inputs: [],
    outputs: [NodeConnectionTypes.Main],
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
        default: [...VK_EVENT_TYPES],
        required: true,
        options: VK_EVENT_TYPES.map((value) => ({
          name: value,
          value,
        })),
        description: 'VK orchestration topics this trigger subscribes to',
      },
      {
        displayName: 'Schema Version',
        name: 'schemaVersion',
        type: 'string',
        default: VK_ORCHESTRATION_SCHEMA_VERSION,
        required: true,
        description: 'Reject MQTT events that do not match this VK event schema version',
      },
    ],
  };

  async trigger(this: ITriggerFunctions): Promise<ITriggerResponse> {
    const eventTypes = this.getNodeParameter('eventTypes') as VkEventType[];
    if (eventTypes.length === 0) {
      throw new NodeOperationError(this.getNode(), 'Select at least one VK event type');
    }

    const schemaVersion = this.getNodeParameter('schemaVersion') as string;
    const credentials =
      await this.getCredentials<VkMqttCredentialValue>('vibeKanbanMqtt');
    const client = await createVkMqttClient(credentials);

    const emitEvent = (event: ReturnType<typeof parseVkOrchestrationEvent>) => {
      this.emit([
        this.helpers.returnJsonArray([
          JSON.parse(JSON.stringify(event)) as IDataObject,
        ]),
      ]);
    };

    const manualTriggerFunction = async () => {
      await new Promise<void>((resolve, reject) => {
        const topics = eventTypes.map(
          (eventType) => `${credentials.topicNamespace.replace(/\/+$/, '')}/${eventType}`,
        );

        const handleManualMessage = (topic: string, payload: Buffer) => {
          try {
            emitEvent(parseVkOrchestrationEvent(payload, topic, schemaVersion));
            client.off('message', handleManualMessage);
            resolve();
          } catch (error) {
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
      await subscribeToVkEvents({
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
      closeFunction: async () => await closeVkMqttClient(client),
      manualTriggerFunction,
    };
  }
}
