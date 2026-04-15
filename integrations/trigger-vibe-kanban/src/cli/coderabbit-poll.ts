import { createDirectDispatcher } from '../trigger/direct-dispatcher';
import { createRuntimeDependencies } from '../runtime/dependencies';
import { runScheduledWorkflowOnce } from '../runtime/scheduled-workflow';

const dependencies = createRuntimeDependencies();

try {
  const result = await runScheduledWorkflowOnce(
    {
      workflowKey: 'coderabbit/poll',
      scopeKey: dependencies.environment.codeRabbitPollScopeKey,
      claimKey: `coderabbit:${new Date().toISOString()}`,
    },
    dependencies,
    createDirectDispatcher(dependencies),
  );

  console.log(JSON.stringify(result));
} finally {
  dependencies.stateStore.close();
}
