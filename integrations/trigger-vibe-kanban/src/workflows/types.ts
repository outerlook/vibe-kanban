import type {
  MqttDispatchInput,
  ScheduledWorkflowDispatchInput,
} from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';

export type OrchestrationWorkflowHandler = {
  workflowKey: string;
  matches(input: MqttDispatchInput): boolean | Promise<boolean>;
  run(input: MqttDispatchInput, deps: RuntimeDependencies): Promise<unknown>;
};

export type ScheduledWorkflowHandler = {
  workflowKey: string;
  run(
    input: ScheduledWorkflowDispatchInput,
    deps: RuntimeDependencies,
  ): Promise<{ output?: unknown; nextCheckpoint?: unknown }>;
};
