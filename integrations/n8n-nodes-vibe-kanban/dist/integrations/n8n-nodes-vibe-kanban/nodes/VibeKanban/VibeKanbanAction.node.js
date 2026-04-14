"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VibeKanbanAction = void 0;
const n8n_workflow_1 = require("n8n-workflow");
const api_1 = require("./shared/api");
const output_1 = require("./shared/output");
function parseAnswersJson(raw) {
    if (!raw.trim()) {
        return [];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
        throw new Error('Answers JSON must be an array of QuestionAnswer objects');
    }
    return parsed;
}
function parseJsonValue(raw, label) {
    try {
        return JSON.parse(raw);
    }
    catch (error) {
        throw new Error(`${label} must be valid JSON: ${error instanceof Error ? error.message : 'parse failed'}`);
    }
}
class VibeKanbanAction {
    description = {
        displayName: 'Vibe Kanban Action',
        name: 'vibeKanbanAction',
        icon: 'file:vibeKanban.svg',
        group: ['transform'],
        version: 1,
        description: 'Invoke VK orchestration command surfaces without generic HTTP glue',
        defaults: {
            name: 'Vibe Kanban Action',
        },
        inputs: [n8n_workflow_1.NodeConnectionTypes.Main],
        outputs: [n8n_workflow_1.NodeConnectionTypes.Main],
        credentials: [
            {
                name: 'vibeKanbanApi',
                required: true,
            },
        ],
        properties: [
            {
                displayName: 'Resource',
                name: 'resource',
                type: 'options',
                default: 'taskSession',
                options: [
                    { name: 'Task', value: 'task' },
                    { name: 'Workspace', value: 'workspace' },
                    { name: 'Task Session', value: 'taskSession' },
                    { name: 'Conversation', value: 'conversation' },
                    { name: 'Approval', value: 'approval' },
                    { name: 'Execution', value: 'execution' },
                    { name: 'Feedback', value: 'feedback' },
                    { name: 'Review Attention', value: 'reviewAttention' },
                ],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'startWorkspaceExecution',
                displayOptions: { show: { resource: ['task'] } },
                options: [{ name: 'Start Workspace Execution', value: 'startWorkspaceExecution' }],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'queueGenerateAndMerge',
                displayOptions: { show: { resource: ['workspace'] } },
                options: [
                    { name: 'Queue Generate And Merge', value: 'queueGenerateAndMerge' },
                    { name: 'Cancel Generate And Merge', value: 'cancelGenerateAndMerge' },
                ],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'startFollowUp',
                displayOptions: { show: { resource: ['taskSession'] } },
                options: [
                    { name: 'Start Follow Up', value: 'startFollowUp' },
                    { name: 'Queue Follow Up', value: 'queueFollowUp' },
                    { name: 'Cancel Queued Follow Up', value: 'cancelQueuedFollowUp' },
                ],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'sendMessage',
                displayOptions: { show: { resource: ['conversation'] } },
                options: [
                    { name: 'Send Message', value: 'sendMessage' },
                    { name: 'Queue Follow Up', value: 'queueFollowUp' },
                    { name: 'Cancel Queued Follow Up', value: 'cancelQueuedFollowUp' },
                ],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'answerApproval',
                displayOptions: { show: { resource: ['approval'] } },
                options: [{ name: 'Answer Approval', value: 'answerApproval' }],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'stopExecution',
                displayOptions: { show: { resource: ['execution'] } },
                options: [{ name: 'Stop Execution', value: 'stopExecution' }],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'createFeedback',
                displayOptions: { show: { resource: ['feedback'] } },
                options: [{ name: 'Create Feedback', value: 'createFeedback' }],
            },
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                default: 'createReviewAttention',
                displayOptions: { show: { resource: ['reviewAttention'] } },
                options: [{ name: 'Create Review Attention', value: 'createReviewAttention' }],
            },
            {
                displayName: 'Task ID',
                name: 'taskId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['task', 'feedback', 'reviewAttention'],
                    },
                },
            },
            {
                displayName: 'Workspace ID',
                name: 'workspaceId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['workspace', 'feedback', 'reviewAttention'],
                    },
                },
            },
            {
                displayName: 'Executor Profile JSON',
                name: 'executorProfileJson',
                type: 'string',
                typeOptions: { rows: 3 },
                default: '{"executor":"CLAUDE_CODE","variant":null}',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['task'],
                        operation: ['startWorkspaceExecution'],
                    },
                },
            },
            {
                displayName: 'Repo Selection',
                name: 'repoSelection',
                type: 'options',
                default: 'taskGroupDefault',
                displayOptions: {
                    show: {
                        resource: ['task'],
                        operation: ['startWorkspaceExecution'],
                    },
                },
                options: [
                    { name: 'Task Group Default', value: 'taskGroupDefault' },
                    { name: 'Explicit Repos JSON', value: 'explicit' },
                ],
            },
            {
                displayName: 'Repos JSON',
                name: 'reposJson',
                type: 'string',
                typeOptions: { rows: 4 },
                default: '[]',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['task'],
                        operation: ['startWorkspaceExecution'],
                        repoSelection: ['explicit'],
                    },
                },
            },
            {
                displayName: 'Repo ID',
                name: 'repoId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['workspace'],
                        operation: ['queueGenerateAndMerge'],
                    },
                },
            },
            {
                displayName: 'Session ID',
                name: 'sessionId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: { show: { resource: ['taskSession'] } },
            },
            {
                displayName: 'Conversation ID',
                name: 'conversationId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: { show: { resource: ['conversation'] } },
            },
            {
                displayName: 'Approval ID',
                name: 'approvalId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: { show: { resource: ['approval'] } },
            },
            {
                displayName: 'Execution Process ID',
                name: 'executionProcessId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['approval', 'execution', 'feedback', 'reviewAttention'],
                    },
                },
            },
            {
                displayName: 'Feedback JSON',
                name: 'feedbackJson',
                type: 'string',
                typeOptions: { rows: 4 },
                default: '{}',
                required: false,
                displayOptions: {
                    show: {
                        resource: ['feedback'],
                        operation: ['createFeedback'],
                    },
                },
            },
            {
                displayName: 'Needs Attention',
                name: 'needsAttention',
                type: 'boolean',
                default: false,
                required: true,
                displayOptions: {
                    show: {
                        resource: ['reviewAttention'],
                        operation: ['createReviewAttention'],
                    },
                },
            },
            {
                displayName: 'Reasoning',
                name: 'reasoning',
                type: 'string',
                typeOptions: { rows: 4 },
                default: '',
                required: false,
                displayOptions: {
                    show: {
                        resource: ['reviewAttention'],
                        operation: ['createReviewAttention'],
                    },
                },
            },
            {
                displayName: 'Prompt',
                name: 'prompt',
                type: 'string',
                typeOptions: { rows: 4 },
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['taskSession'],
                        operation: ['startFollowUp'],
                    },
                },
            },
            {
                displayName: 'Message',
                name: 'message',
                type: 'string',
                typeOptions: { rows: 4 },
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        operation: ['queueFollowUp'],
                    },
                },
            },
            {
                displayName: 'Content',
                name: 'content',
                type: 'string',
                typeOptions: { rows: 4 },
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['conversation'],
                        operation: ['sendMessage'],
                    },
                },
            },
            {
                displayName: 'Variant',
                name: 'variant',
                type: 'string',
                default: '',
                required: false,
                description: 'Optional VK executor variant',
                displayOptions: {
                    show: {
                        operation: ['startFollowUp', 'queueFollowUp', 'sendMessage'],
                    },
                },
            },
            {
                displayName: 'Retry Process ID',
                name: 'retryProcessId',
                type: 'string',
                default: '',
                required: false,
                displayOptions: {
                    show: {
                        resource: ['taskSession'],
                        operation: ['startFollowUp'],
                    },
                },
            },
            {
                displayName: 'Force When Dirty',
                name: 'forceWhenDirty',
                type: 'boolean',
                default: false,
                displayOptions: {
                    show: {
                        resource: ['taskSession'],
                        operation: ['startFollowUp'],
                    },
                },
            },
            {
                displayName: 'Perform Git Reset',
                name: 'performGitReset',
                type: 'boolean',
                default: false,
                displayOptions: {
                    show: {
                        resource: ['taskSession'],
                        operation: ['startFollowUp'],
                    },
                },
            },
            {
                displayName: 'Approval Response',
                name: 'approvalResponse',
                type: 'options',
                default: 'approved',
                displayOptions: { show: { resource: ['approval'] } },
                options: [
                    { name: 'Approved', value: 'approved' },
                    { name: 'Denied', value: 'denied' },
                    { name: 'Answered', value: 'answered' },
                    { name: 'Timed Out', value: 'timed_out' },
                ],
            },
            {
                displayName: 'Deny Reason',
                name: 'denyReason',
                type: 'string',
                default: '',
                displayOptions: {
                    show: {
                        resource: ['approval'],
                        approvalResponse: ['denied'],
                    },
                },
            },
            {
                displayName: 'Answers JSON',
                name: 'answersJson',
                type: 'string',
                typeOptions: { rows: 4 },
                default: '[]',
                description: 'JSON array of QuestionAnswer objects for answered user questions',
                displayOptions: {
                    show: {
                        resource: ['approval'],
                        approvalResponse: ['answered'],
                    },
                },
            },
        ],
    };
    async execute() {
        const credentials = await this.getCredentials('vibeKanbanApi');
        const inputItems = this.getInputData();
        const itemCount = Math.max(inputItems.length, 1);
        const returnData = [];
        for (let itemIndex = 0; itemIndex < itemCount; itemIndex++) {
            try {
                const resource = this.getNodeParameter('resource', itemIndex);
                const operation = this.getNodeParameter('operation', itemIndex);
                let output;
                switch (resource) {
                    case 'task': {
                        const taskId = this.getNodeParameter('taskId', itemIndex);
                        const executorProfileJson = this.getNodeParameter('executorProfileJson', itemIndex);
                        const repoSelection = this.getNodeParameter('repoSelection', itemIndex);
                        const executorProfile = parseJsonValue(executorProfileJson, 'Executor Profile JSON');
                        const command = repoSelection === 'explicit'
                            ? {
                                task_id: taskId,
                                executor_profile_id: executorProfile,
                                repo_selection: 'explicit',
                                repos: parseJsonValue(this.getNodeParameter('reposJson', itemIndex), 'Repos JSON'),
                            }
                            : {
                                task_id: taskId,
                                executor_profile_id: executorProfile,
                                repo_selection: 'task_group_default',
                            };
                        const data = await (0, api_1.startWorkspaceExecution)(credentials, command);
                        output = (0, output_1.normalizeActionOutput)({
                            resource,
                            operation,
                            identifiers: { taskId },
                            data,
                        });
                        break;
                    }
                    case 'workspace': {
                        const workspaceId = this.getNodeParameter('workspaceId', itemIndex);
                        if (operation === 'queueGenerateAndMerge') {
                            const repoId = this.getNodeParameter('repoId', itemIndex);
                            const data = await (0, api_1.queueGenerateAndMerge)(credentials, workspaceId, {
                                repo_id: repoId,
                            });
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { workspaceId, repoId },
                                data,
                            });
                        }
                        else {
                            await (0, api_1.cancelGenerateAndMerge)(credentials, workspaceId);
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { workspaceId },
                            });
                        }
                        break;
                    }
                    case 'taskSession': {
                        const sessionId = this.getNodeParameter('sessionId', itemIndex);
                        if (operation === 'startFollowUp') {
                            const prompt = this.getNodeParameter('prompt', itemIndex);
                            const variant = this.getNodeParameter('variant', itemIndex, '');
                            const retryProcessId = this.getNodeParameter('retryProcessId', itemIndex, '');
                            const forceWhenDirty = this.getNodeParameter('forceWhenDirty', itemIndex);
                            const performGitReset = this.getNodeParameter('performGitReset', itemIndex);
                            const data = await (0, api_1.startTaskFollowUp)(credentials, sessionId, {
                                prompt,
                                variant: variant || undefined,
                                retry_process_id: retryProcessId || undefined,
                                force_when_dirty: forceWhenDirty,
                                perform_git_reset: performGitReset,
                            });
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { sessionId },
                                data,
                            });
                        }
                        else if (operation === 'queueFollowUp') {
                            const message = this.getNodeParameter('message', itemIndex);
                            const variant = this.getNodeParameter('variant', itemIndex, '');
                            const data = await (0, api_1.queueTaskFollowUp)(credentials, sessionId, {
                                message,
                                variant: variant || undefined,
                            });
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { sessionId },
                                data,
                            });
                        }
                        else {
                            const data = await (0, api_1.cancelTaskFollowUp)(credentials, sessionId);
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { sessionId },
                                data,
                            });
                        }
                        break;
                    }
                    case 'conversation': {
                        const conversationId = this.getNodeParameter('conversationId', itemIndex);
                        if (operation === 'sendMessage') {
                            const content = this.getNodeParameter('content', itemIndex);
                            const variant = this.getNodeParameter('variant', itemIndex, '');
                            const data = await (0, api_1.sendConversationMessage)(credentials, conversationId, {
                                content,
                                variant: variant || undefined,
                            });
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { conversationId },
                                data,
                            });
                        }
                        else if (operation === 'queueFollowUp') {
                            const message = this.getNodeParameter('message', itemIndex);
                            const variant = this.getNodeParameter('variant', itemIndex, '');
                            const data = await (0, api_1.queueConversationFollowUp)(credentials, conversationId, {
                                message,
                                variant: variant || undefined,
                            });
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { conversationId },
                                data,
                            });
                        }
                        else {
                            const data = await (0, api_1.cancelConversationFollowUp)(credentials, conversationId);
                            output = (0, output_1.normalizeActionOutput)({
                                resource,
                                operation,
                                identifiers: { conversationId },
                                data,
                            });
                        }
                        break;
                    }
                    case 'approval': {
                        const approvalId = this.getNodeParameter('approvalId', itemIndex);
                        const executionProcessId = this.getNodeParameter('executionProcessId', itemIndex);
                        const approvalResponse = this.getNodeParameter('approvalResponse', itemIndex);
                        let body;
                        if (approvalResponse === 'approved') {
                            body = {
                                execution_process_id: executionProcessId,
                                status: { status: 'approved' },
                            };
                        }
                        else if (approvalResponse === 'denied') {
                            const reason = this.getNodeParameter('denyReason', itemIndex, '');
                            body = {
                                execution_process_id: executionProcessId,
                                status: { status: 'denied', reason: reason || undefined },
                            };
                        }
                        else if (approvalResponse === 'answered') {
                            const answersJson = this.getNodeParameter('answersJson', itemIndex);
                            const answers = parseAnswersJson(answersJson);
                            body = {
                                execution_process_id: executionProcessId,
                                status: { status: 'answered', answers },
                                answers,
                            };
                        }
                        else {
                            body = {
                                execution_process_id: executionProcessId,
                                status: { status: 'timed_out' },
                            };
                        }
                        const data = await (0, api_1.answerApproval)(credentials, approvalId, body);
                        output = (0, output_1.normalizeActionOutput)({
                            resource,
                            operation,
                            identifiers: { approvalId, executionProcessId },
                            data,
                        });
                        break;
                    }
                    case 'execution': {
                        const executionProcessId = this.getNodeParameter('executionProcessId', itemIndex);
                        await (0, api_1.stopExecutionProcess)(credentials, executionProcessId);
                        output = (0, output_1.normalizeActionOutput)({
                            resource,
                            operation,
                            identifiers: { executionProcessId },
                        });
                        break;
                    }
                    case 'feedback': {
                        const executionProcessId = this.getNodeParameter('executionProcessId', itemIndex);
                        const taskId = this.getNodeParameter('taskId', itemIndex);
                        const workspaceId = this.getNodeParameter('workspaceId', itemIndex);
                        const feedbackJson = this.getNodeParameter('feedbackJson', itemIndex, '');
                        const feedback = feedbackJson.trim()
                            ? JSON.stringify(parseJsonValue(feedbackJson, 'Feedback JSON'))
                            : undefined;
                        const data = await (0, api_1.createFeedback)(credentials, {
                            execution_process_id: executionProcessId,
                            task_id: taskId,
                            workspace_id: workspaceId,
                            feedback_json: feedback ?? null,
                        });
                        output = (0, output_1.normalizeActionOutput)({
                            resource,
                            operation,
                            identifiers: { executionProcessId, taskId, workspaceId },
                            data,
                        });
                        break;
                    }
                    case 'reviewAttention': {
                        const executionProcessId = this.getNodeParameter('executionProcessId', itemIndex);
                        const taskId = this.getNodeParameter('taskId', itemIndex);
                        const workspaceId = this.getNodeParameter('workspaceId', itemIndex);
                        const needsAttention = this.getNodeParameter('needsAttention', itemIndex);
                        const reasoning = this.getNodeParameter('reasoning', itemIndex, '');
                        const data = await (0, api_1.createReviewAttention)(credentials, {
                            execution_process_id: executionProcessId,
                            task_id: taskId,
                            workspace_id: workspaceId,
                            needs_attention: needsAttention,
                            reasoning: reasoning || null,
                        });
                        output = (0, output_1.normalizeActionOutput)({
                            resource,
                            operation,
                            identifiers: { executionProcessId, taskId, workspaceId },
                            data,
                        });
                        break;
                    }
                    default:
                        throw new Error(`Unsupported VK action resource '${resource}'`);
                }
                returnData.push({
                    json: output,
                    pairedItem: inputItems[itemIndex] ? { item: itemIndex } : undefined,
                });
            }
            catch (error) {
                if (this.continueOnFail()) {
                    returnData.push({
                        json: {
                            error: error instanceof Error ? error.message : 'Unknown VK action error',
                        },
                        pairedItem: inputItems[itemIndex] ? { item: itemIndex } : undefined,
                    });
                    continue;
                }
                throw new n8n_workflow_1.NodeOperationError(this.getNode(), error, { itemIndex });
            }
        }
        return [returnData];
    }
}
exports.VibeKanbanAction = VibeKanbanAction;
//# sourceMappingURL=VibeKanbanAction.node.js.map