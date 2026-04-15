import type {
  VkApprovalContext,
  VkApprovalStatus,
  VkConversationContext,
  VkCreateConversationResponse,
  VkExecutionContext,
  VkFeedbackResponse,
  VkFollowUpResult,
  VkGenerateCommitMessageResponse,
  VkQueueMergeResult,
  VkQueueStatus,
  VkReadResource,
  VkReviewAttention,
  VkSelectedProject,
  VkSelectedGitHubRepository,
  VkSelectResource,
  VkSendMessageResponse,
  VkStartTaskExecutionResult,
  VkTaskContext,
  VkTaskGroupContext,
} from "./vk-contracts";

type VkReadOutput =
  | VkTaskContext
  | VkTaskGroupContext
  | VkConversationContext
  | VkExecutionContext
  | VkApprovalContext;

type VkSelectOutput = VkSelectedProject | VkSelectedGitHubRepository;

export function normalizeReadOutput(
  resource: VkReadResource,
  data: VkReadOutput,
) {
  return {
    resource,
    ...data,
    _meta: {
      resource,
      surface: "orchestration-context",
    },
  };
}

export function normalizeSelectOutput(
  resource: VkSelectResource,
  data: VkSelectOutput,
) {
  return {
    resource,
    ...data,
    _meta: {
      resource,
      surface: "selection",
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
    | VkGenerateCommitMessageResponse
    | VkQueueMergeResult
    | VkQueueStatus
    | VkReviewAttention
    | VkSendMessageResponse
    | VkStartTaskExecutionResult;
}) {
  const base = {
    resource: args.resource,
    operation: args.operation,
    ...args.identifiers,
  };

  if (args.operation === "startFollowUp" && args.data) {
    const followUp = args.data as VkFollowUpResult;
    return {
      ...base,
      followUp,
      executionProcess:
        followUp.status === "started" ? followUp.execution_process : null,
      queueEntry: followUp.status === "queued" ? followUp.queue_entry : null,
    };
  }

  if (args.operation === "startTaskExecution" && args.data) {
    const taskExecution = args.data as VkStartTaskExecutionResult;
    return {
      ...base,
      taskExecution,
      workspace: taskExecution.workspace,
      workspaceResolution: taskExecution.workspace_resolution,
      executorProfileId: taskExecution.executor_profile_id,
      executionProcess:
        taskExecution.status === "started"
          ? taskExecution.execution_process
          : null,
      queueEntry:
        taskExecution.status === "queued" ? taskExecution.queue_entry : null,
    };
  }

  if (
    args.operation === "queueFollowUp" ||
    args.operation === "cancelQueuedFollowUp"
  ) {
    return {
      ...base,
      queue: args.data,
    };
  }

  if (args.operation === "createConversation" && args.data) {
    const conversationCreation = args.data as VkCreateConversationResponse;
    return {
      ...base,
      conversationCreation,
      conversation: conversationCreation.session,
      initialMessage: conversationCreation.initial_message,
      executionProcessId: conversationCreation.execution_process_id,
    };
  }

  if (args.operation === "sendMessage" && args.data) {
    const message = args.data as VkSendMessageResponse;
    return {
      ...base,
      message,
      userMessage: message.user_message,
      executionProcessId: message.execution_process_id,
    };
  }

  if (args.operation === "generateCommitMessage" && args.data) {
    const result = args.data as VkGenerateCommitMessageResponse;
    return {
      ...base,
      commitMessageResponse: result,
      commitMessage: result.commit_message,
    };
  }

  if (args.operation === "queueMerge" && args.data) {
    const result = args.data as VkQueueMergeResult;
    return {
      ...base,
      mergeQueueResult: result,
      mergeQueueEntry: result.status === "queued" ? result.entry : null,
      mergeQueueError: result.status === "rejected" ? result.error : null,
    };
  }

  if (args.operation === "answerApproval") {
    return {
      ...base,
      approvalStatus: args.data,
    };
  }

  if (args.operation === "stopExecution") {
    return {
      ...base,
      stopped: true,
    };
  }

  if (args.operation === "cancelQueueMerge") {
    return {
      ...base,
      cancelled: true,
    };
  }

  return {
    ...base,
    result: args.data ?? null,
  };
}
