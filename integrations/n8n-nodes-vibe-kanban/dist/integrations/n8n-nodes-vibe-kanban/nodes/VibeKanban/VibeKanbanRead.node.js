"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VibeKanbanRead = void 0;
const n8n_workflow_1 = require("n8n-workflow");
const api_1 = require("./shared/api");
const output_1 = require("./shared/output");
class VibeKanbanRead {
    description = {
        displayName: 'Vibe Kanban Read',
        name: 'vibeKanbanRead',
        icon: 'file:vibeKanban.svg',
        group: ['transform'],
        version: 1,
        description: 'Hydrate VK orchestration context through first-class read surfaces',
        defaults: {
            name: 'Vibe Kanban Read',
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
                default: 'task',
                options: [
                    { name: 'Task Context', value: 'task' },
                    { name: 'Task Group Context', value: 'taskGroup' },
                    { name: 'Conversation Context', value: 'conversation' },
                    { name: 'Execution Context', value: 'execution' },
                    { name: 'Approval Context', value: 'approval' },
                ],
            },
            {
                displayName: 'Project ID',
                name: 'projectId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['task'],
                    },
                },
            },
            {
                displayName: 'Task ID',
                name: 'taskId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['task'],
                    },
                },
            },
            {
                displayName: 'Task Group ID',
                name: 'taskGroupId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['taskGroup'],
                    },
                },
            },
            {
                displayName: 'Conversation ID',
                name: 'conversationId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['conversation'],
                    },
                },
            },
            {
                displayName: 'Execution Process ID',
                name: 'executionProcessId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['execution'],
                    },
                },
            },
            {
                displayName: 'Approval ID',
                name: 'approvalId',
                type: 'string',
                default: '',
                required: true,
                displayOptions: {
                    show: {
                        resource: ['approval'],
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
                let data;
                switch (resource) {
                    case 'task': {
                        const projectId = this.getNodeParameter('projectId', itemIndex);
                        const taskId = this.getNodeParameter('taskId', itemIndex);
                        data = await (0, api_1.getTaskContext)(credentials, projectId, taskId);
                        break;
                    }
                    case 'taskGroup': {
                        const taskGroupId = this.getNodeParameter('taskGroupId', itemIndex);
                        data = await (0, api_1.getTaskGroupContext)(credentials, taskGroupId);
                        break;
                    }
                    case 'conversation': {
                        const conversationId = this.getNodeParameter('conversationId', itemIndex);
                        data = await (0, api_1.getConversationContext)(credentials, conversationId);
                        break;
                    }
                    case 'execution': {
                        const executionProcessId = this.getNodeParameter('executionProcessId', itemIndex);
                        data = await (0, api_1.getExecutionContext)(credentials, executionProcessId);
                        break;
                    }
                    case 'approval': {
                        const approvalId = this.getNodeParameter('approvalId', itemIndex);
                        data = await (0, api_1.getApprovalContext)(credentials, approvalId);
                        break;
                    }
                    default:
                        throw new Error(`Unsupported VK read resource '${resource}'`);
                }
                returnData.push({
                    json: (0, output_1.normalizeReadOutput)(resource, data),
                    pairedItem: inputItems[itemIndex] ? { item: itemIndex } : undefined,
                });
            }
            catch (error) {
                if (this.continueOnFail()) {
                    returnData.push({
                        json: {
                            error: error instanceof Error ? error.message : 'Unknown VK read error',
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
exports.VibeKanbanRead = VibeKanbanRead;
//# sourceMappingURL=VibeKanbanRead.node.js.map