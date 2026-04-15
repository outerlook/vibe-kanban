import { connect, type IClientOptions, type MqttClient } from 'mqtt';

import {
  ORCHESTRATION_EVENT_TYPES,
  parseOrchestrationEvent,
  topicForOrchestrationEvent,
  type OrchestrationTriggerItem,
} from '../../../../shared/orchestration-events';

import type { OrchestratorDispatcher } from './contracts';
import type { RuntimeDependencies } from './dependencies';
import { createDefaultMqttDispatcher } from '../trigger/trigger-dispatcher';

type BridgeArgs = {
  dependencies: RuntimeDependencies;
  dispatcher?: OrchestratorDispatcher;
  eventTypes?: string[];
  client?: MqttClient;
};

function createMqttClientOptions(dependencies: RuntimeDependencies): IClientOptions {
  const { vkMqtt } = dependencies.environment;
  return {
    ...(vkMqtt.username ? { username: vkMqtt.username } : {}),
    ...(vkMqtt.password ? { password: vkMqtt.password } : {}),
    ...(vkMqtt.clientId ? { clientId: vkMqtt.clientId } : {}),
    clean: vkMqtt.clean ?? true,
  };
}

async function connectClient(
  dependencies: RuntimeDependencies,
  client?: MqttClient,
): Promise<MqttClient> {
  if (client) {
    return client;
  }

  return await new Promise<MqttClient>((resolve, reject) => {
    const mqttClient = connect(
      dependencies.environment.vkMqtt.brokerUrl,
      createMqttClientOptions(dependencies),
    );

    const onConnect = () => {
      mqttClient.off('error', onError);
      resolve(mqttClient);
    };

    const onError = (error: Error) => {
      mqttClient.off('connect', onConnect);
      mqttClient.end(true);
      reject(error);
    };

    mqttClient.once('connect', onConnect);
    mqttClient.once('error', onError);
  });
}

async function subscribeTopics(client: MqttClient, topics: string[]): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    client.subscribe(topics, { qos: 0 }, (error) => {
      if (error) {
        reject(error);
        return;
      }

      resolve();
    });
  });
}

async function handleEvent(
  event: OrchestrationTriggerItem,
  dependencies: RuntimeDependencies,
  dispatcher: OrchestratorDispatcher,
): Promise<void> {
  const workflowKey = `mqtt/${event.eventType}`;
  const claimResult = dependencies.stateStore.claimEventId({
    claimKey: event.eventId,
    workflowKey,
    scopeKey: event.topic,
    metadata: {
      eventType: event.eventType,
      topic: event.topic,
      occurredAt: event.occurredAt,
    },
  });

  if (!claimResult.claimed) {
    dependencies.logger.info('Skipping duplicate MQTT event claim', {
      eventId: event.eventId,
      eventType: event.eventType,
    });
    return;
  }

  dependencies.stateStore.markClaimDispatched(claimResult.record.claimKey, {
    workflowKey,
    eventType: event.eventType,
  });

  try {
    const contexts = await dependencies.vkClient.hydrateOrchestrationContexts(event);
    const result = await dispatcher.dispatchOrchestrationEvent({
      claim: claimResult.record,
      event,
      contexts,
    });

    dependencies.stateStore.markClaimCompleted(claimResult.record.claimKey, {
      workflowKey,
      disposition: result.disposition,
      handlerKeys: result.handlerKeys,
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : 'MQTT orchestration dispatch failed';
    dependencies.stateStore.markClaimFailed(claimResult.record.claimKey, message);
    throw error;
  }
}

export async function startMqttBridgeRuntime(
  args: BridgeArgs,
): Promise<{ client: MqttClient; close(): Promise<void> }> {
  const dependencies = args.dependencies;
  const dispatcher = args.dispatcher ?? createDefaultMqttDispatcher(dependencies);
  const client = await connectClient(dependencies, args.client);
  const eventTypes = args.eventTypes ?? dependencies.environment.eventTypes ?? [...ORCHESTRATION_EVENT_TYPES];
  const topics = eventTypes.map((eventType) =>
    topicForOrchestrationEvent(dependencies.environment.vkMqtt.topicNamespace, eventType as never),
  );

  await subscribeTopics(client, topics);

  const onMessage = async (topic: string, payload: Buffer) => {
    try {
      const event = parseOrchestrationEvent(
        payload,
        topic,
        dependencies.environment.schemaVersion,
      );
      await handleEvent(event, dependencies, dispatcher);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'MQTT runtime failed';
      dependencies.logger.error('Failed to process MQTT orchestration event', {
        topic,
        error: message,
      });
    }
  };

  client.on('message', onMessage);

  dependencies.logger.info('MQTT bridge subscribed', {
    topicNamespace: dependencies.environment.vkMqtt.topicNamespace,
    eventTypes,
  });

  return {
    client,
    async close() {
      client.off('message', onMessage);
      await new Promise<void>((resolve) => {
        client.end(true, {}, () => resolve());
      });
    },
  };
}
