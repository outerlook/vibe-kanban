import type {
  ApprovalResponse,
  ApprovalStatus,
  BaseCodingAgent,
  CreateConversationRequest,
  CreateConversationResponse,
  CreateAgentFeedback,
  CreateReviewAttention,
  ExecutorConfigs,
  ExecutorProfileId,
  FeedbackResponse,
  FollowUpResult,
  GenerateCommitMessageRequest,
  GenerateCommitMessageResponse,
  OrchestrationApprovalContextDto,
  OrchestrationConversationContextDto,
  OrchestrationEventEnvelope,
  OrchestrationExecutionContextDto,
  OrchestrationTaskContextDto,
  OrchestrationTaskGroupContextDto,
  ProjectGitHubRepository,
  ProjectWithTaskCounts,
  MergeQueueEntry,
  QueueMergeError,
  QuestionAnswer,
  QueueStatus,
  QueueMergeRequest,
  ReviewAttention,
  SendMessageResponse,
  StartTaskExecutionCommand,
  StartTaskExecutionResult,
  WorkflowAssociation,
} from "../../../generated/shared-types";

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
export type VkBaseCodingAgent = BaseCodingAgent;
export type VkExecutorConfigs = ExecutorConfigs;
export type VkExecutorProfileId = ExecutorProfileId;
export type VkCreateConversationRequest = CreateConversationRequest;
export type VkCreateConversationResponse = CreateConversationResponse;
export type VkStartTaskExecutionCommand = StartTaskExecutionCommand;
export type VkStartTaskExecutionResult = StartTaskExecutionResult;
export type VkGenerateCommitMessageRequest = GenerateCommitMessageRequest;
export type VkGenerateCommitMessageResponse = GenerateCommitMessageResponse;
export type VkMergeQueueEntry = MergeQueueEntry;
export type VkQueueMergeRequest = QueueMergeRequest;
export type VkQueueMergeError = QueueMergeError;
export type VkQueueMergeResult =
  | { status: "queued"; entry: MergeQueueEntry }
  | { status: "rejected"; error: QueueMergeError };
export type VkCreateAgentFeedback = CreateAgentFeedback;
export type VkCreateReviewAttention = CreateReviewAttention;
export type VkFeedbackResponse = FeedbackResponse;
export type VkReviewAttention = ReviewAttention;
export type VkProject = ProjectWithTaskCounts;
export type VkWorkflowAssociation = WorkflowAssociation | null;
export type VkProjectGitHubRepository = ProjectGitHubRepository;
export type VkSelectedGitHubRepository = ProjectGitHubRepository & {
  projectIds: string[];
  projectNames: string[];
};

export type VkReadResource =
  | "task"
  | "taskGroup"
  | "conversation"
  | "execution"
  | "approval"
  | "githubRepositories";

export type VkActionResource =
  | "task"
  | "workspace"
  | "taskSession"
  | "conversation"
  | "approval"
  | "execution"
  | "feedback"
  | "reviewAttention";

export type VkApiCredentialValue = {
  baseUrl: string;
  authMode: "none" | "bearerToken" | "customHeader";
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
