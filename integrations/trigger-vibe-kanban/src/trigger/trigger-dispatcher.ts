import { configure, tasks } from '@trigger.dev/sdk';

import type {
  DispatchResult,
  MqttDispatchInput,
  OrchestratorDispatcher,
  ScheduledDispatchResult,
  ScheduledWorkflowDispatchInput,
} from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';
import { createDirectDispatcher } from './direct-dispatcher';
import { dispatchScheduledWorkflow } from '../workflows/registry';
import { ORCHESTRATION_EVENT_TASK_ID } from './task-ids';

type TriggerTaskInvoker = typeof tasks.trigger;

function configureTriggerApiClient(): void {
  const accessToken = process.env.TRIGGER_SECRET_KEY?.trim();
  if (!accessToken) {
    throw new Error(
      'Trigger-backed MQTT routing requires TRIGGER_SECRET_KEY so the bridge can enqueue Trigger task runs.',
    );
  }

  configure({
    accessToken,
    ...(process.env.TRIGGER_API_URL?.trim()
      ? { baseURL: process.env.TRIGGER_API_URL.trim() }
      : {}),
  });
}

export function createTriggerDispatcher(
  dependencies: RuntimeDependencies,
  triggerTask: TriggerTaskInvoker = tasks.trigger,
): OrchestratorDispatcher {
  configureTriggerApiClient();

  return {
    async dispatchOrchestrationEvent(
      input: MqttDispatchInput,
    ): Promise<DispatchResult> {
      const handle = await triggerTask(
        ORCHESTRATION_EVENT_TASK_ID,
        input,
        {
          idempotencyKey: [
            'vk-mqtt-event',
            input.event.eventType,
            input.event.eventId,
          ],
          concurrencyKey: `vk-mqtt:${input.event.eventType}`,
          tags: [
            `vk-event:${input.event.eventType}`,
            `vk-claim:${input.claim.claimKey}`,
          ],
        },
      );

      dependencies.logger.info('Enqueued Trigger task for MQTT event', {
        eventId: input.event.eventId,
        eventType: input.event.eventType,
        runId: handle.id,
      });

      return {
        disposition: 'handled',
        handlerKeys: [ORCHESTRATION_EVENT_TASK_ID],
        output: {
          route: 'trigger',
          runId: handle.id,
        },
      };
    },

    async dispatchScheduledWorkflow(
      input: ScheduledWorkflowDispatchInput,
    ): Promise<ScheduledDispatchResult> {
      return dispatchScheduledWorkflow(input, dependencies);
    },
  };
}

export function createDefaultMqttDispatcher(
  dependencies: RuntimeDependencies,
): OrchestratorDispatcher {
  if (dependencies.environment.mqttRouterMode === 'direct') {
    return createDirectDispatcher(dependencies);
  }

  return createTriggerDispatcher(dependencies);
}
