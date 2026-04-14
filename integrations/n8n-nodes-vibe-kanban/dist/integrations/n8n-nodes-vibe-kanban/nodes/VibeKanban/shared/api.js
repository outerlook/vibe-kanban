"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.getTaskContext = getTaskContext;
exports.getTaskGroupContext = getTaskGroupContext;
exports.getConversationContext = getConversationContext;
exports.createConversation = createConversation;
exports.getExecutionContext = getExecutionContext;
exports.getApprovalContext = getApprovalContext;
exports.startTaskFollowUp = startTaskFollowUp;
exports.queueTaskFollowUp = queueTaskFollowUp;
exports.cancelTaskFollowUp = cancelTaskFollowUp;
exports.sendConversationMessage = sendConversationMessage;
exports.queueConversationFollowUp = queueConversationFollowUp;
exports.cancelConversationFollowUp = cancelConversationFollowUp;
exports.answerApproval = answerApproval;
exports.startWorkspaceExecution = startWorkspaceExecution;
exports.queueGenerateAndMerge = queueGenerateAndMerge;
exports.cancelGenerateAndMerge = cancelGenerateAndMerge;
exports.createFeedback = createFeedback;
exports.createReviewAttention = createReviewAttention;
exports.stopExecutionProcess = stopExecutionProcess;
function normalizeBaseUrl(baseUrl) {
    return `${baseUrl.replace(/\/+$/, '')}/api`;
}
function buildHeaders(credentials) {
    const headers = {
        'content-type': 'application/json',
    };
    if (credentials.authMode === 'bearerToken' && credentials.token) {
        const prefix = credentials.headerPrefix ?? 'Bearer ';
        headers.Authorization = `${prefix}${credentials.token}`;
    }
    if (credentials.authMode === 'customHeader' &&
        credentials.token &&
        credentials.headerName) {
        headers[credentials.headerName] = credentials.token;
    }
    return headers;
}
async function requestVk(credentials, path, init) {
    const response = await fetch(`${normalizeBaseUrl(credentials.baseUrl)}${path}`, {
        ...init,
        headers: {
            ...buildHeaders(credentials),
            ...(init?.headers ?? {}),
        },
    });
    const text = await response.text();
    const body = text.length > 0 ? JSON.parse(text) : null;
    if (!response.ok) {
        throw new Error(body?.message ?? `VK request failed with HTTP ${response.status}`);
    }
    if (!body?.success || body.data === undefined) {
        throw new Error(body?.message ?? 'VK control plane returned an unexpected response');
    }
    return body.data;
}
async function getTaskContext(credentials, projectId, taskId) {
    return requestVk(credentials, `/projects/${projectId}/tasks/${taskId}/orchestration-context`);
}
async function getTaskGroupContext(credentials, taskGroupId) {
    return requestVk(credentials, `/task-groups/${taskGroupId}/orchestration-context`);
}
async function getConversationContext(credentials, conversationId) {
    return requestVk(credentials, `/conversations/${conversationId}/orchestration-context`);
}
async function createConversation(credentials, projectId, body) {
    return requestVk(credentials, `/projects/${projectId}/conversations`, {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function getExecutionContext(credentials, executionProcessId) {
    return requestVk(credentials, `/execution-processes/${executionProcessId}/orchestration-context`);
}
async function getApprovalContext(credentials, approvalId) {
    return requestVk(credentials, `/approvals/${approvalId}/orchestration-context`);
}
async function startTaskFollowUp(credentials, sessionId, body) {
    return requestVk(credentials, `/sessions/${sessionId}/follow-up`, {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function queueTaskFollowUp(credentials, sessionId, body) {
    return requestVk(credentials, `/sessions/${sessionId}/queue`, {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function cancelTaskFollowUp(credentials, sessionId) {
    return requestVk(credentials, `/sessions/${sessionId}/queue`, {
        method: 'DELETE',
    });
}
async function sendConversationMessage(credentials, conversationId, body) {
    return requestVk(credentials, `/conversations/${conversationId}/messages`, {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function queueConversationFollowUp(credentials, conversationId, body) {
    return requestVk(credentials, `/conversations/${conversationId}/queue`, {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function cancelConversationFollowUp(credentials, conversationId) {
    return requestVk(credentials, `/conversations/${conversationId}/queue`, {
        method: 'DELETE',
    });
}
async function answerApproval(credentials, approvalId, body) {
    const response = await fetch(`${normalizeBaseUrl(credentials.baseUrl)}/approvals/${approvalId}/respond`, {
        method: 'POST',
        headers: buildHeaders(credentials),
        body: JSON.stringify(body),
    });
    if (!response.ok) {
        throw new Error(`VK approval response failed with HTTP ${response.status}`);
    }
    return (await response.json());
}
async function startWorkspaceExecution(credentials, body) {
    return requestVk(credentials, '/task-attempts/orchestration/workspace-executions', {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function queueGenerateAndMerge(credentials, workspaceId, body) {
    return requestVk(credentials, `/task-attempts/${workspaceId}/generate-and-merge`, {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function cancelGenerateAndMerge(credentials, workspaceId) {
    return requestVk(credentials, `/task-attempts/${workspaceId}/generate-and-merge`, {
        method: 'DELETE',
    });
}
async function createFeedback(credentials, body) {
    return requestVk(credentials, '/feedback', {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function createReviewAttention(credentials, body) {
    return requestVk(credentials, '/review-attention', {
        method: 'POST',
        body: JSON.stringify(body),
    });
}
async function stopExecutionProcess(credentials, executionProcessId) {
    return requestVk(credentials, `/execution-processes/${executionProcessId}/stop`, {
        method: 'POST',
    });
}
//# sourceMappingURL=api.js.map