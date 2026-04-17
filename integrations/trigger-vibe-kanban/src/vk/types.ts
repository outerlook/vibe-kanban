import type { VkApiConfig } from '../../../../shared/vk-api-client';
import type {
  ApprovalResponse,
  ApprovalStatus,
  ConversationMessageMetadata,
  ConversationStructuredOutputMetadata,
  CreateAgentFeedback,
  CreateConversationRequest,
  CreateConversationResponse,
  CreateFollowUpAttempt,
  CreateReviewAttention,
  DraftFollowUpData,
  FeedbackResponse,
  FollowUpResult,
  GenerateCommitMessageRequest,
  GenerateCommitMessageResponse,
  MergeQueueEntry,
  OrchestrationApprovalContextDto,
  OrchestrationConversationContextDto,
  OrchestrationExecutionContextDto,
  OrchestrationTaskContextDto,
  OrchestrationTaskGroupContextDto,
  ProjectGitHubRepository,
  ProjectWithTaskCounts,
  QueueMergeError,
  QueueMergeRequest,
  QueueStatus,
  ReviewAttention,
  SendMessageRequest,
  SendMessageResponse,
  StartTaskExecutionCommand,
  StartTaskExecutionResult,
  NormalizedEntry,
  StructuredOutputContract,
  StructuredOutputSchemaValidationIssue,
  StructuredOutputValidationErrorMetadata,
  StructuredOutputValidationStatus,
  TaskListItemDto,
  WorkflowAssociation,
} from '../../../../shared/types';

export type VkMqttConfig = {
  brokerUrl: string;
  topicNamespace: string;
  username?: string;
  password?: string;
  clientId?: string;
  clean?: boolean;
};

export type HydratedOrchestrationContexts = {
  task: OrchestrationTaskContextDto | null;
  taskGroup: OrchestrationTaskGroupContextDto | null;
  conversation: OrchestrationConversationContextDto | null;
  execution: OrchestrationExecutionContextDto | null;
  approval: OrchestrationApprovalContextDto | null;
};

export type QueueMergeResult =
  | { status: 'queued'; entry: MergeQueueEntry }
  | { status: 'rejected'; error: QueueMergeError };

export type ExecutionProcessNormalizedEntryRecord = {
  entry_index: number;
  entry: NormalizedEntry;
};

export type ExecutionProcessNormalizedEntriesPage = {
  entries: ExecutionProcessNormalizedEntryRecord[];
  next_before_index: number | null;
  has_more: boolean;
};

export const EXECUTION_HISTORY_RECAP_ENTRY_BUDGET = 400;

export type ExecutionHistoryRecap = {
  entries: ExecutionProcessNormalizedEntryRecord[];
  totalEntries: number;
  droppedEntries: number;
  truncated: boolean;
  budget: {
    maxEntries: typeof EXECUTION_HISTORY_RECAP_ENTRY_BUDGET;
    truncation: 'drop_oldest';
  };
};

export type {
  ApprovalResponse,
  ApprovalStatus,
  ConversationMessageMetadata,
  ConversationStructuredOutputMetadata,
  CreateAgentFeedback,
  CreateConversationRequest,
  CreateConversationResponse,
  CreateFollowUpAttempt,
  CreateReviewAttention,
  DraftFollowUpData,
  FeedbackResponse,
  FollowUpResult,
  GenerateCommitMessageRequest,
  GenerateCommitMessageResponse,
  MergeQueueEntry,
  OrchestrationApprovalContextDto,
  OrchestrationConversationContextDto,
  OrchestrationExecutionContextDto,
  OrchestrationTaskContextDto,
  OrchestrationTaskGroupContextDto,
  ProjectGitHubRepository,
  NormalizedEntry,
  ProjectWithTaskCounts,
  QueueMergeError,
  QueueMergeRequest,
  QueueStatus,
  ReviewAttention,
  SendMessageRequest,
  SendMessageResponse,
  StartTaskExecutionCommand,
  StartTaskExecutionResult,
  StructuredOutputContract,
  StructuredOutputSchemaValidationIssue,
  StructuredOutputValidationErrorMetadata,
  StructuredOutputValidationStatus,
  TaskListItemDto,
  VkApiConfig,
  WorkflowAssociation,
};
