import { task } from '@trigger.dev/sdk';

import type {
  OpenClawConversationCleanupRequest,
  OpenClawConversationRunRequest,
} from '../runtime/contracts';
import { cleanupOpenClawConversationRequest, runOpenClawConversationRequest } from '../runtime/openclaw-conversation-runner';
import { createRuntimeDependencies } from '../runtime/dependencies';
import {
  OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
  OPENCLAW_CONVERSATION_TASK_ID,
} from './task-ids';

export const openClawConversationTask = task({
  id: OPENCLAW_CONVERSATION_TASK_ID,
  run: async (payload: OpenClawConversationRunRequest) => {
    const dependencies = createRuntimeDependencies();

    try {
      return await runOpenClawConversationRequest(payload, dependencies);
    } finally {
      dependencies.stateStore.close();
    }
  },
});

export const openClawConversationCleanupTask = task({
  id: OPENCLAW_CONVERSATION_CLEANUP_TASK_ID,
  run: async (payload: OpenClawConversationCleanupRequest) => {
    const dependencies = createRuntimeDependencies();

    try {
      return await cleanupOpenClawConversationRequest(payload, dependencies);
    } finally {
      dependencies.stateStore.close();
    }
  },
});
