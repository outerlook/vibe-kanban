import type {
  ApprovalResponse,
  ApprovalStatus,
  CreateConversationRequest,
  CreateConversationResponse,
  CreateAgentFeedback,
  CreateReviewAttention,
  ExecutorProfileId,
  FeedbackResponse,
  FollowUpResult,
  OrchestrationApprovalContextDto,
  OrchestrationConversationContextDto,
  OrchestrationEventEnvelope,
  OrchestrationExecutionContextDto,
  OrchestrationTaskContextDto,
  OrchestrationTaskGroupContextDto,
  QueueGenerateAndMergeCommand,
  QueueGenerateAndMergeResult,
  QuestionAnswer,
  QueueStatus,
  ReviewAttention,
  SendMessageResponse,
  StartWorkspaceExecutionCommand,
  StartWorkspaceExecutionResult,
} from '../../../../../shared/types';

export type VkOrchestrationEventEnvelope = OrchestrationEventEnvelope;
export type VkTaskContext = OrchestrationTaskContextDto;
export type VkTaskGroupContext = OrchestrationTaskGroupContextDto;
export type VkConversationContext = OrchestrationConversationContextDto;
export type VkExecutionContext = OrchestrationExecutionContextDto;
export type VkApprovalContext = OrchestrationApprovalContextDto;
export type VkFollowUpResult = FollowUpResult;
export type VkQueueStatus = QueueStatus;
export type VkSendMessageResponse = SendMessageResponse;
export type VkApprovalResponse = ApprovalResponse;
export type VkApprovalStatus = ApprovalStatus;
export type VkQuestionAnswer = QuestionAnswer;
export type VkExecutorProfileId = ExecutorProfileId;
export type VkCreateConversationRequest = CreateConversationRequest;
export type VkCreateConversationResponse = CreateConversationResponse;
export type VkStartWorkspaceExecutionCommand = StartWorkspaceExecutionCommand;
export type VkStartWorkspaceExecutionResult = StartWorkspaceExecutionResult;
export type VkQueueGenerateAndMergeCommand = QueueGenerateAndMergeCommand;
export type VkQueueGenerateAndMergeResult = QueueGenerateAndMergeResult;
export type VkCreateAgentFeedback = CreateAgentFeedback;
export type VkCreateReviewAttention = CreateReviewAttention;
export type VkFeedbackResponse = FeedbackResponse;
export type VkReviewAttention = ReviewAttention;

export type VkReadResource =
  | 'task'
  | 'taskGroup'
  | 'conversation'
  | 'execution'
  | 'approval';

export type VkActionResource =
  | 'task'
  | 'workspace'
  | 'taskSession'
  | 'conversation'
  | 'approval'
  | 'execution'
  | 'feedback'
  | 'reviewAttention';

export type VkApiCredentialValue = {
  baseUrl: string;
  authMode: 'none' | 'bearerToken' | 'customHeader';
  token?: string;
  headerName?: string;
  headerPrefix?: string;
};

export type VkMqttCredentialValue = {
  brokerUrl: string;
  topicNamespace: string;
  username?: string;
  password?: string;
  clientId?: string;
  clean?: boolean;
};
