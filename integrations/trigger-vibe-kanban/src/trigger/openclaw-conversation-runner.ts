import { tasks } from '@trigger.dev/sdk';

import type {
  OpenClawConversationCleanupRequest,
  OpenClawConversationRequest,
  OpenClawConversationRunRequest,
} from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';
import { configureTriggerApiClient } from './trigger-api';
import {
  OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
  OPENCLAW_CONVERSATION_TASK_ID,
} from './task-ids';

type TriggerTaskInvoker = typeof tasks.trigger;

export type TriggerOpenClawConversationRunHandle = {
  runId: string;
  taskId: string;
};

export type TriggerOpenClawConversationRunner = {
  run(
    request: OpenClawConversationRunRequest,
  ): Promise<TriggerOpenClawConversationRunHandle>;
  cleanup(
    request: OpenClawConversationCleanupRequest,
  ): Promise<TriggerOpenClawConversationRunHandle>;
};

function buildCommonTriggerOptions(request: OpenClawConversationRequest) {
  return {
    idempotencyKey: [
      'openclaw-conversation',
      request.action,
      request.correlation.idempotencyKey,
    ],
    concurrencyKey: `openclaw:${request.correlation.correlationKey}`,
    tags: [
      `workflow:${request.correlation.workflowKey}`,
      `scope:${request.correlation.scopeKey}`,
      `correlation:${request.correlation.correlationKey}`,
    ],
  };
}

export function createTriggerOpenClawConversationRunner(
  _dependencies: RuntimeDependencies,
  triggerTask: TriggerTaskInvoker = tasks.trigger,
): TriggerOpenClawConversationRunner {
  configureTriggerApiClient();

  return {
    async run(request) {
      const handle = await triggerTask(
        OPENCLAW_CONVERSATION_TASK_ID,
        request,
        buildCommonTriggerOptions(request),
      );

      return {
        runId: handle.id,
        taskId: OPENCLAW_CONVERSATION_TASK_ID,
      };
    },
    async cleanup(request) {
      const handle = await triggerTask(
        OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
        request,
        buildCommonTriggerOptions(request),
      );

      return {
        runId: handle.id,
        taskId: OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
      };
    },
  };
}
