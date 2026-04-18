import type { OrchestrationTriggerItem } from '../../../../shared/orchestration-events';

import type {
  ClaimRecord,
  JsonValue,
  WorkflowCheckpointRecord,
} from '../state/types';
import type { HydratedOrchestrationContexts } from '../vk/types';
import type {
  TriggerExecutorOperationKey,
  TriggerExecutorProfileName,
} from './executor-mapping';
import type { TriggerOpenClawProfileName } from './openclaw-profile-mapping';

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

export type OpenClawConversationCleanupPolicy = 'preserve' | 'delete';

export type OpenClawConversationCorrelation = {
  workflowKey: string;
  scopeKey: string;
  correlationKey: string;
  idempotencyKey: string;
};

export type OpenClawConversationSelection =
  | {
      profileName: TriggerOpenClawProfileName | TriggerExecutorProfileName;
      operationKey?: never;
    }
  | {
      profileName?: never;
      operationKey: TriggerExecutorOperationKey;
    };

export type OpenClawConversationTextResponseMode = {
  kind: 'text';
};

export type OpenClawConversationStructuredResponseMode = {
  kind: 'structured';
  schema: JsonValue;
};

export type OpenClawConversationResponseMode =
  | OpenClawConversationTextResponseMode
  | OpenClawConversationStructuredResponseMode;

export type OpenClawConversationRunRequest = {
  action: 'run';
  prompt: string;
  response: OpenClawConversationResponseMode;
  workingDirectory: string;
  selection: OpenClawConversationSelection;
  correlation: OpenClawConversationCorrelation;
  timeoutMs: number;
  cleanup: {
    onSuccess: OpenClawConversationCleanupPolicy;
    onError: OpenClawConversationCleanupPolicy;
  };
  session?: {
    key?: string;
  };
};

export type OpenClawConversationCleanupRequest = {
  action: 'cleanup';
  correlation: OpenClawConversationCorrelation;
  session?: {
    key?: string;
  };
};

export type OpenClawConversationRequest =
  | OpenClawConversationRunRequest
  | OpenClawConversationCleanupRequest;

export type OpenClawConversationRunMetadata = {
  claimKey: string;
  idempotencyKey: string;
  durationMs: number;
  agentRunId: string | null;
  selectedModel: {
    provider: string;
    model: string;
    thinkingLevel?: string;
  } | null;
};

export type OpenClawConversationSessionMetadata = {
  key: string;
  cleanedUp: boolean;
};

export type OpenClawConversationSelectionMetadata = {
  profileName: TriggerOpenClawProfileName | TriggerExecutorProfileName;
  engineModel: string;
  modelRef: string;
};

export type OpenClawConversationTextResult = {
  kind: 'text';
  text: string;
};

export type OpenClawConversationStructuredResult = {
  kind: 'structured';
  text: string;
  value: JsonValue;
};

export type OpenClawConversationRunResult = {
  action: 'run';
  session: OpenClawConversationSessionMetadata;
  selection: OpenClawConversationSelectionMetadata;
  response:
    | OpenClawConversationTextResult
    | OpenClawConversationStructuredResult;
  run: OpenClawConversationRunMetadata;
};

export type OpenClawConversationCleanupResult = {
  action: 'cleanup';
  disposition: 'cleaned' | 'already-cleaned';
  session: OpenClawConversationSessionMetadata;
};

export type OpenClawConversationResult =
  | OpenClawConversationRunResult
  | OpenClawConversationCleanupResult;

export interface OpenClawConversationExecutor {
  run(
    request: OpenClawConversationRunRequest,
  ): Promise<OpenClawConversationRunResult>;
}

export interface OrchestratorDispatcher {
  dispatchOrchestrationEvent(input: MqttDispatchInput): Promise<DispatchResult>;
  dispatchScheduledWorkflow(
    input: ScheduledWorkflowDispatchInput,
  ): Promise<ScheduledDispatchResult>;
}
