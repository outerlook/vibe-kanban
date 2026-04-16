import {
  buildCodeRabbitPollMetadataFromEnv,
} from '../coderabbit/poller';
import { createRuntimeDependencies } from '../runtime/dependencies';
import { runScheduledWorkflowOnce } from '../runtime/scheduled-workflow';
import { createDirectDispatcher } from '../trigger/direct-dispatcher';

const dependencies = createRuntimeDependencies();

try {
  const result = await runScheduledWorkflowOnce(
    {
      workflowKey: 'coderabbit/poll',
      scopeKey: dependencies.environment.codeRabbitPollScopeKey,
      claimKey: `coderabbit:${new Date().toISOString()}`,
      metadata: buildCodeRabbitPollMetadataFromEnv(),
    },
    dependencies,
    createDirectDispatcher(dependencies),
  );

  console.log(JSON.stringify(result));
} finally {
  dependencies.stateStore.close();
}
