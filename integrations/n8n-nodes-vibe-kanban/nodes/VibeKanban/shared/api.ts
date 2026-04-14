import type {
  VkApiCredentialValue,
  VkApprovalContext,
  VkApprovalResponse,
  VkApprovalStatus,
  VkConversationContext,
  VkCreateConversationRequest,
  VkCreateConversationResponse,
  VkCreateAgentFeedback,
  VkCreateReviewAttention,
  VkExecutionContext,
  VkFeedbackResponse,
  VkFollowUpResult,
  VkQueueGenerateAndMergeCommand,
  VkQueueGenerateAndMergeResult,
  VkQueueStatus,
  VkReviewAttention,
  VkSendMessageResponse,
  VkStartWorkspaceExecutionCommand,
  VkStartWorkspaceExecutionResult,
  VkTaskContext,
  VkTaskGroupContext,
} from './vk-contracts';

type ApiEnvelope<T> = {
  success: boolean;
  data?: T;
  error_data?: unknown;
  message?: string;
};

function normalizeBaseUrl(baseUrl: string): string {
  return `${baseUrl.replace(/\/+$/, '')}/api`;
}

function buildHeaders(credentials: VkApiCredentialValue): Record<string, string> {
  const headers: Record<string, string> = {
    'content-type': 'application/json',
  };

  if (credentials.authMode === 'bearerToken' && credentials.token) {
    const prefix = credentials.headerPrefix ?? 'Bearer ';
    headers.Authorization = `${prefix}${credentials.token}`;
  }

  if (
    credentials.authMode === 'customHeader' &&
    credentials.token &&
    credentials.headerName
  ) {
    headers[credentials.headerName] = credentials.token;
  }

  return headers;
}

async function requestVk<T>(
  credentials: VkApiCredentialValue,
  path: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(`${normalizeBaseUrl(credentials.baseUrl)}${path}`, {
    ...init,
    headers: {
      ...buildHeaders(credentials),
      ...(init?.headers ?? {}),
    },
  });

  const text = await response.text();
  const body = text.length > 0 ? (JSON.parse(text) as ApiEnvelope<T>) : null;

  if (!response.ok) {
    throw new Error(body?.message ?? `VK request failed with HTTP ${response.status}`);
  }

  if (!body?.success || body.data === undefined) {
    throw new Error(body?.message ?? 'VK control plane returned an unexpected response');
  }

  return body.data;
}

export async function getTaskContext(
  credentials: VkApiCredentialValue,
  projectId: string,
  taskId: string,
): Promise<VkTaskContext> {
  return requestVk<VkTaskContext>(
    credentials,
    `/projects/${projectId}/tasks/${taskId}/orchestration-context`,
  );
}

export async function getTaskGroupContext(
  credentials: VkApiCredentialValue,
  taskGroupId: string,
): Promise<VkTaskGroupContext> {
  return requestVk<VkTaskGroupContext>(
    credentials,
    `/task-groups/${taskGroupId}/orchestration-context`,
  );
}

export async function getConversationContext(
  credentials: VkApiCredentialValue,
  conversationId: string,
): Promise<VkConversationContext> {
  return requestVk<VkConversationContext>(
    credentials,
    `/conversations/${conversationId}/orchestration-context`,
  );
}

export async function createConversation(
  credentials: VkApiCredentialValue,
  projectId: string,
  body: VkCreateConversationRequest,
): Promise<VkCreateConversationResponse> {
  return requestVk<VkCreateConversationResponse>(
    credentials,
    `/projects/${projectId}/conversations`,
    {
      method: 'POST',
      body: JSON.stringify(body),
    },
  );
}

export async function getExecutionContext(
  credentials: VkApiCredentialValue,
  executionProcessId: string,
): Promise<VkExecutionContext> {
  return requestVk<VkExecutionContext>(
    credentials,
    `/execution-processes/${executionProcessId}/orchestration-context`,
  );
}

export async function getApprovalContext(
  credentials: VkApiCredentialValue,
  approvalId: string,
): Promise<VkApprovalContext> {
  return requestVk<VkApprovalContext>(
    credentials,
    `/approvals/${approvalId}/orchestration-context`,
  );
}

export async function startTaskFollowUp(
  credentials: VkApiCredentialValue,
  sessionId: string,
  body: {
    prompt: string;
    variant?: string;
    retry_process_id?: string;
    force_when_dirty?: boolean;
    perform_git_reset?: boolean;
  },
): Promise<VkFollowUpResult> {
  return requestVk<VkFollowUpResult>(credentials, `/sessions/${sessionId}/follow-up`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export async function queueTaskFollowUp(
  credentials: VkApiCredentialValue,
  sessionId: string,
  body: { message: string; variant?: string },
): Promise<VkQueueStatus> {
  return requestVk<VkQueueStatus>(credentials, `/sessions/${sessionId}/queue`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export async function cancelTaskFollowUp(
  credentials: VkApiCredentialValue,
  sessionId: string,
): Promise<VkQueueStatus> {
  return requestVk<VkQueueStatus>(credentials, `/sessions/${sessionId}/queue`, {
    method: 'DELETE',
  });
}

export async function sendConversationMessage(
  credentials: VkApiCredentialValue,
  conversationId: string,
  body: { content: string; variant?: string },
): Promise<VkSendMessageResponse> {
  return requestVk<VkSendMessageResponse>(
    credentials,
    `/conversations/${conversationId}/messages`,
    {
      method: 'POST',
      body: JSON.stringify(body),
    },
  );
}

export async function queueConversationFollowUp(
  credentials: VkApiCredentialValue,
  conversationId: string,
  body: { message: string; variant?: string },
): Promise<VkQueueStatus> {
  return requestVk<VkQueueStatus>(credentials, `/conversations/${conversationId}/queue`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export async function cancelConversationFollowUp(
  credentials: VkApiCredentialValue,
  conversationId: string,
): Promise<VkQueueStatus> {
  return requestVk<VkQueueStatus>(credentials, `/conversations/${conversationId}/queue`, {
    method: 'DELETE',
  });
}

export async function answerApproval(
  credentials: VkApiCredentialValue,
  approvalId: string,
  body: VkApprovalResponse,
): Promise<VkApprovalStatus> {
  const response = await fetch(
    `${normalizeBaseUrl(credentials.baseUrl)}/approvals/${approvalId}/respond`,
    {
      method: 'POST',
      headers: buildHeaders(credentials),
      body: JSON.stringify(body),
    },
  );

  if (!response.ok) {
    throw new Error(`VK approval response failed with HTTP ${response.status}`);
  }

  return (await response.json()) as VkApprovalStatus;
}

export async function startWorkspaceExecution(
  credentials: VkApiCredentialValue,
  body: VkStartWorkspaceExecutionCommand,
): Promise<VkStartWorkspaceExecutionResult> {
  return requestVk<VkStartWorkspaceExecutionResult>(
    credentials,
    '/task-attempts/orchestration/workspace-executions',
    {
      method: 'POST',
      body: JSON.stringify(body),
    },
  );
}

export async function queueGenerateAndMerge(
  credentials: VkApiCredentialValue,
  workspaceId: string,
  body: VkQueueGenerateAndMergeCommand,
): Promise<VkQueueGenerateAndMergeResult> {
  return requestVk<VkQueueGenerateAndMergeResult>(
    credentials,
    `/task-attempts/${workspaceId}/generate-and-merge`,
    {
      method: 'POST',
      body: JSON.stringify(body),
    },
  );
}

export async function cancelGenerateAndMerge(
  credentials: VkApiCredentialValue,
  workspaceId: string,
): Promise<void> {
  return requestVk<void>(
    credentials,
    `/task-attempts/${workspaceId}/generate-and-merge`,
    {
      method: 'DELETE',
    },
  );
}

export async function createFeedback(
  credentials: VkApiCredentialValue,
  body: VkCreateAgentFeedback,
): Promise<VkFeedbackResponse> {
  return requestVk<VkFeedbackResponse>(credentials, '/feedback', {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export async function createReviewAttention(
  credentials: VkApiCredentialValue,
  body: VkCreateReviewAttention,
): Promise<VkReviewAttention> {
  return requestVk<VkReviewAttention>(credentials, '/review-attention', {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export async function stopExecutionProcess(
  credentials: VkApiCredentialValue,
  executionProcessId: string,
) {
  return requestVk<void>(credentials, `/execution-processes/${executionProcessId}/stop`, {
    method: 'POST',
  });
}
