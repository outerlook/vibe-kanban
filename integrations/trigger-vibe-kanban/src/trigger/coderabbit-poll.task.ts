import { schedules } from '@trigger.dev/sdk';

import {
  buildCodeRabbitPollMetadataFromEnv,
} from '../coderabbit/poller';
import { createRuntimeDependencies } from '../runtime/dependencies';
import { runScheduledWorkflowOnce } from '../runtime/scheduled-workflow';
import { createDirectDispatcher } from './direct-dispatcher';
import { CODERABBIT_POLL_TASK_ID } from './task-ids';

const coderabbitCron = process.env.CODERABBIT_POLL_CRON;

export const coderabbitPollTask = schedules.task({
  id: CODERABBIT_POLL_TASK_ID,
  ...(coderabbitCron
    ? {
        cron: {
          pattern: coderabbitCron,
        },
      }
    : {}),
  run: async (payload) => {
    const dependencies = createRuntimeDependencies();

    try {
      return await runScheduledWorkflowOnce(
        {
          workflowKey: 'coderabbit/poll',
          scopeKey: payload.externalId ?? dependencies.environment.codeRabbitPollScopeKey,
          claimKey: `${payload.scheduleId}:${payload.timestamp.toISOString()}`,
          scheduledAt: payload.timestamp.toISOString(),
          metadata: buildCodeRabbitPollMetadataFromEnv(),
        },
        dependencies,
        createDirectDispatcher(dependencies),
      );
    } finally {
      dependencies.stateStore.close();
    }
  },
});
