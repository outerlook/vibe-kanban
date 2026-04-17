import { tasks } from '@trigger.dev/sdk';

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
import { configureTriggerApiClient } from './trigger-api';

type TriggerTaskInvoker = typeof tasks.trigger;

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
