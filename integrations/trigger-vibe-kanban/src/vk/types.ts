import type { VkApiConfig } from '../../../../shared/vk-api-client';
import type {
  ApprovalResponse,
  ApprovalStatus,
  CreateAgentFeedback,
  CreateConversationRequest,
  CreateConversationResponse,
  CreateReviewAttention,
  FeedbackResponse,
  FollowUpResult,
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
  SendMessageResponse,
  StartTaskExecutionCommand,
  StartTaskExecutionResult,
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

export type {
  ApprovalResponse,
  ApprovalStatus,
  CreateAgentFeedback,
  CreateConversationRequest,
  CreateConversationResponse,
  CreateReviewAttention,
  FeedbackResponse,
  FollowUpResult,
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
  SendMessageResponse,
  StartTaskExecutionCommand,
  StartTaskExecutionResult,
  VkApiConfig,
  WorkflowAssociation,
};
