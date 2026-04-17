import { task } from '@trigger.dev/sdk';

import type { MqttDispatchInput } from '../runtime/contracts';
import { createRuntimeDependencies } from '../runtime/dependencies';
import { dispatchOrchestrationEvent } from '../workflows/registry';
import { buildTriggerOpenClawConversationOptions } from './openclaw-conversation-runner';
import { openClawConversationTask } from './openclaw-conversation.task';
import { ORCHESTRATION_EVENT_TASK_ID } from './task-ids';

export const orchestrationEventTask = task({
  id: ORCHESTRATION_EVENT_TASK_ID,
  run: async (payload: MqttDispatchInput) => {
    const dependencies = createRuntimeDependencies();
    dependencies.openClawConversationExecutor = {
      async run(request) {
        return openClawConversationTask
          .triggerAndWait(
            request,
            buildTriggerOpenClawConversationOptions(request),
          )
          .unwrap();
      },
    };

    try {
      return await dispatchOrchestrationEvent(payload, dependencies);
    } finally {
      dependencies.stateStore.close();
    }
  },
});
