import type { OrchestrationTriggerItem } from '../../../../shared/orchestration-events';

import { VkHttpClient } from './http-client';
import type {
  ApprovalResponse,
  ApprovalStatus,
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
  HydratedOrchestrationContexts,
  MergeQueueEntry,
  OrchestrationApprovalContextDto,
  OrchestrationConversationContextDto,
  OrchestrationExecutionContextDto,
  OrchestrationTaskContextDto,
  OrchestrationTaskGroupContextDto,
  QueueMergeError,
  QueueMergeRequest,
  QueueMergeResult,
  QueueStatus,
  ReviewAttention,
  SendMessageRequest,
  SendMessageResponse,
  StartTaskExecutionCommand,
  StartTaskExecutionResult,
  VkApiConfig,
} from './types';

export class VkRuntimeClient extends VkHttpClient {
  constructor(config: VkApiConfig) {
    super(config);
  }

  async getTaskContext(taskId: string): Promise<OrchestrationTaskContextDto> {
    return this.request(`/tasks/${taskId}/orchestration-context`);
  }

  async getTaskGroupContext(
    taskGroupId: string,
  ): Promise<OrchestrationTaskGroupContextDto> {
    return this.request(`/task-groups/${taskGroupId}/orchestration-context`);
  }

  async getConversationContext(
    conversationId: string,
  ): Promise<OrchestrationConversationContextDto> {
    return this.request(`/conversations/${conversationId}/orchestration-context`);
  }

  async getExecutionContext(
    executionProcessId: string,
  ): Promise<OrchestrationExecutionContextDto> {
    return this.request(
      `/execution-processes/${executionProcessId}/orchestration-context`,
    );
  }

  async getApprovalContext(
    approvalId: string,
  ): Promise<OrchestrationApprovalContextDto> {
    return this.request(`/approvals/${approvalId}/orchestration-context`);
  }

  async hydrateOrchestrationContexts(
    event: OrchestrationTriggerItem,
  ): Promise<HydratedOrchestrationContexts> {
    const [task, taskGroup, conversation, execution, approval] = await Promise.all([
      event.refs.taskContext
        ? this.getTaskContext(event.refs.taskContext.taskId)
        : Promise.resolve(null),
      event.refs.taskGroupContext
        ? this.getTaskGroupContext(event.refs.taskGroupContext.taskGroupId)
        : Promise.resolve(null),
      event.refs.conversationContext
        ? this.getConversationContext(event.refs.conversationContext.conversationId)
        : Promise.resolve(null),
      event.refs.executionContext
        ? this.getExecutionContext(event.refs.executionContext.executionProcessId)
        : Promise.resolve(null),
      event.refs.approvalContext
        ? this.getApprovalContext(event.refs.approvalContext.approvalId)
        : Promise.resolve(null),
    ]);

    return {
      task,
      taskGroup,
      conversation,
      execution,
      approval,
    };
  }

  async createConversation(
    projectId: string,
    body: CreateConversationRequest,
  ): Promise<CreateConversationResponse> {
    return this.request(`/projects/${projectId}/conversations`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async answerApproval(
    approvalId: string,
    body: ApprovalResponse,
  ): Promise<ApprovalStatus> {
    return this.requestRawJson(`/approvals/${approvalId}/respond`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async startTaskFollowUp(
    sessionId: string,
    body: CreateFollowUpAttempt,
  ): Promise<FollowUpResult> {
    return this.request(`/sessions/${sessionId}/follow-up`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async queueTaskFollowUp(
    sessionId: string,
    body: DraftFollowUpData,
  ): Promise<QueueStatus> {
    return this.request(`/sessions/${sessionId}/queue`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async cancelTaskFollowUp(sessionId: string): Promise<QueueStatus> {
    return this.request(`/sessions/${sessionId}/queue`, {
      method: 'DELETE',
    });
  }

  async sendConversationMessage(
    conversationId: string,
    body: SendMessageRequest,
  ): Promise<SendMessageResponse> {
    return this.request(`/conversations/${conversationId}/messages`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async queueConversationFollowUp(
    conversationId: string,
    body: DraftFollowUpData,
  ): Promise<QueueStatus> {
    return this.request(`/conversations/${conversationId}/queue`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async cancelConversationFollowUp(
    conversationId: string,
  ): Promise<QueueStatus> {
    return this.request(`/conversations/${conversationId}/queue`, {
      method: 'DELETE',
    });
  }

  async startTaskExecution(
    body: StartTaskExecutionCommand,
  ): Promise<StartTaskExecutionResult> {
    return this.request('/task-attempts/orchestration/task-executions', {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async generateCommitMessage(
    workspaceId: string,
    body: GenerateCommitMessageRequest,
  ): Promise<GenerateCommitMessageResponse> {
    return this.request(`/task-attempts/${workspaceId}/generate-commit-message`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async queueMerge(
    workspaceId: string,
    body: QueueMergeRequest,
  ): Promise<QueueMergeResult> {
    const envelope = await this.requestEnvelope<MergeQueueEntry, QueueMergeError>(
      `/task-attempts/${workspaceId}/queue-merge`,
      {
        method: 'POST',
        body: JSON.stringify(body),
      },
    );

    if (envelope.success && envelope.data) {
      return {
        status: 'queued',
        entry: envelope.data,
      };
    }

    if (envelope.error_data) {
      return {
        status: 'rejected',
        error: envelope.error_data,
      };
    }

    throw new Error(envelope.message ?? 'VK queue merge returned an unexpected response');
  }

  async cancelQueueMerge(workspaceId: string): Promise<void> {
    await this.request<void>(`/task-attempts/${workspaceId}/queue-merge`, {
      method: 'DELETE',
    });
  }

  async createFeedback(body: CreateAgentFeedback): Promise<FeedbackResponse> {
    return this.request('/feedback', {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async createReviewAttention(body: CreateReviewAttention): Promise<ReviewAttention> {
    return this.request('/review-attention', {
      method: 'POST',
      body: JSON.stringify(body),
    });
  }

  async stopExecutionProcess(executionProcessId: string): Promise<void> {
    await this.request<void>(`/execution-processes/${executionProcessId}/stop`, {
      method: 'POST',
    });
  }
}
