import type { OrchestratorDispatcher } from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';
import {
  dispatchOrchestrationEvent,
  dispatchScheduledWorkflow,
} from '../workflows/registry';

export function createDirectDispatcher(
  dependencies: RuntimeDependencies,
): OrchestratorDispatcher {
  return {
    dispatchOrchestrationEvent(input) {
      return dispatchOrchestrationEvent(input, dependencies);
    },
    dispatchScheduledWorkflow(input) {
      return dispatchScheduledWorkflow(input, dependencies);
    },
  };
}
