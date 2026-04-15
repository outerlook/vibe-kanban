import type { OrchestrationTriggerItem } from '../../../../shared/orchestration-events';

import type {
  ClaimRecord,
  JsonValue,
  WorkflowCheckpointRecord,
} from '../state/types';
import type { HydratedOrchestrationContexts } from '../vk/types';

export type OrchestratorLogger = {
  debug(message: string, context?: JsonValue): void;
  info(message: string, context?: JsonValue): void;
  warn(message: string, context?: JsonValue): void;
  error(message: string, context?: JsonValue): void;
};

export type MqttDispatchInput = {
  claim: ClaimRecord;
  event: OrchestrationTriggerItem;
  contexts: HydratedOrchestrationContexts;
};

export type ScheduledWorkflowDispatchInput = {
  workflowKey: string;
  scopeKey: string;
  scheduledAt: string;
  claim: ClaimRecord;
  checkpoint: WorkflowCheckpointRecord | null;
};

export type DispatchResult = {
  disposition: 'handled' | 'skipped';
  handlerKeys: string[];
  output: JsonValue | null;
};

export type ScheduledDispatchResult = DispatchResult & {
  nextCheckpoint: JsonValue | null | undefined;
};

export interface OrchestratorDispatcher {
  dispatchOrchestrationEvent(input: MqttDispatchInput): Promise<DispatchResult>;
  dispatchScheduledWorkflow(
    input: ScheduledWorkflowDispatchInput,
  ): Promise<ScheduledDispatchResult>;
}
