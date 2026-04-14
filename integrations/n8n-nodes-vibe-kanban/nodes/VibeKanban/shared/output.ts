import type {
  VkApprovalContext,
  VkApprovalStatus,
  VkConversationContext,
  VkCreateConversationResponse,
  VkExecutionContext,
  VkFeedbackResponse,
  VkFollowUpResult,
  VkQueueGenerateAndMergeResult,
  VkQueueStatus,
  VkReadResource,
  VkReviewAttention,
  VkSendMessageResponse,
  VkStartWorkspaceExecutionResult,
  VkTaskContext,
  VkTaskGroupContext,
} from './vk-contracts';

type VkReadOutput =
  | VkTaskContext
  | VkTaskGroupContext
  | VkConversationContext
  | VkExecutionContext
  | VkApprovalContext;

export function normalizeReadOutput(resource: VkReadResource, data: VkReadOutput) {
  return {
    resource,
    ...data,
    _meta: {
      resource,
      surface: 'orchestration-context',
    },
  };
}

export function normalizeActionOutput(args: {
  resource: string;
  operation: string;
  identifiers: Record<string, string>;
  data?:
    | VkApprovalStatus
    | VkCreateConversationResponse
    | VkFeedbackResponse
    | VkFollowUpResult
    | VkQueueGenerateAndMergeResult
    | VkQueueStatus
    | VkReviewAttention
    | VkSendMessageResponse
    | VkStartWorkspaceExecutionResult;
}) {
  const base = {
    resource: args.resource,
    operation: args.operation,
    ...args.identifiers,
  };

  if (args.operation === 'startFollowUp' && args.data) {
    const followUp = args.data as VkFollowUpResult;
    return {
      ...base,
      followUp,
      executionProcess:
        followUp.status === 'started'
          ? followUp.execution_process
          : null,
      queueEntry:
        followUp.status === 'queued'
          ? followUp.queue_entry
          : null,
    };
  }

  if (
    args.operation === 'queueFollowUp' ||
    args.operation === 'cancelQueuedFollowUp'
  ) {
    return {
      ...base,
      queue: args.data,
    };
  }

  if (args.operation === 'createConversation' && args.data) {
    const conversationCreation = args.data as VkCreateConversationResponse;
    return {
      ...base,
      conversationCreation,
      conversation: conversationCreation.session,
      initialMessage: conversationCreation.initial_message,
      executionProcessId: conversationCreation.execution_process_id,
    };
  }

  if (args.operation === 'sendMessage' && args.data) {
    const message = args.data as VkSendMessageResponse;
    return {
      ...base,
      message,
      userMessage: message.user_message,
      executionProcessId: message.execution_process_id,
    };
  }

  if (args.operation === 'answerApproval') {
    return {
      ...base,
      approvalStatus: args.data,
    };
  }

  if (args.operation === 'stopExecution') {
    return {
      ...base,
      stopped: true,
    };
  }

  return {
    ...base,
    result: args.data ?? null,
  };
}
