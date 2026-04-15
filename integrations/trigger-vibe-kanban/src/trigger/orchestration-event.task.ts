import { task } from '@trigger.dev/sdk';

import type { MqttDispatchInput } from '../runtime/contracts';
import { createRuntimeDependencies } from '../runtime/dependencies';
import { dispatchOrchestrationEvent } from '../workflows/registry';
import { ORCHESTRATION_EVENT_TASK_ID } from './task-ids';

export const orchestrationEventTask = task({
  id: ORCHESTRATION_EVENT_TASK_ID,
  run: async (payload: MqttDispatchInput) => {
    const dependencies = createRuntimeDependencies();

    try {
      return await dispatchOrchestrationEvent(payload, dependencies);
    } finally {
      dependencies.stateStore.close();
    }
  },
});
