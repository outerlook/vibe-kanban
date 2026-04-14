import type {
  ApprovalResponse,
  ApprovalStatus,
  FollowUpResult,
  OrchestrationApprovalContextDto,
  OrchestrationConversationContextDto,
  OrchestrationEventEnvelope,
  OrchestrationExecutionContextDto,
  OrchestrationTaskContextDto,
  OrchestrationTaskGroupContextDto,
  QuestionAnswer,
  QueueStatus,
  SendMessageResponse,
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

export type VkReadResource =
  | 'task'
  | 'taskGroup'
  | 'conversation'
  | 'execution'
  | 'approval';

export type VkActionResource =
  | 'taskSession'
  | 'conversation'
  | 'approval'
  | 'execution';

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
