import type {
  IExecuteFunctions,
  INodeExecutionData,
  INodeType,
  INodeTypeDescription,
} from 'n8n-workflow';
import { NodeConnectionTypes, NodeOperationError } from 'n8n-workflow';

import {
  getApprovalContext,
  getConversationContext,
  getExecutionContext,
  getTaskContext,
  getTaskGroupContext,
} from './shared/api';
import { normalizeReadOutput } from './shared/output';
import type { VkApiCredentialValue, VkReadResource } from './shared/vk-contracts';

export class VibeKanbanRead implements INodeType {
  description: INodeTypeDescription = {
    displayName: 'Vibe Kanban Read',
    name: 'vibeKanbanRead',
    icon: 'file:vibeKanban.svg',
    group: ['transform'],
    version: 1,
    description: 'Hydrate VK orchestration context through first-class read surfaces',
    defaults: {
      name: 'Vibe Kanban Read',
    },
    inputs: [NodeConnectionTypes.Main],
    outputs: [NodeConnectionTypes.Main],
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

  async execute(this: IExecuteFunctions): Promise<INodeExecutionData[][]> {
    const credentials = await this.getCredentials<VkApiCredentialValue>('vibeKanbanApi');
    const inputItems = this.getInputData();
    const itemCount = Math.max(inputItems.length, 1);
    const returnData: INodeExecutionData[] = [];

    for (let itemIndex = 0; itemIndex < itemCount; itemIndex++) {
      try {
        const resource = this.getNodeParameter(
          'resource',
          itemIndex,
        ) as VkReadResource;

        let data;
        switch (resource) {
          case 'task': {
            const projectId = this.getNodeParameter('projectId', itemIndex) as string;
            const taskId = this.getNodeParameter('taskId', itemIndex) as string;
            data = await getTaskContext(credentials, projectId, taskId);
            break;
          }
          case 'taskGroup': {
            const taskGroupId = this.getNodeParameter('taskGroupId', itemIndex) as string;
            data = await getTaskGroupContext(credentials, taskGroupId);
            break;
          }
          case 'conversation': {
            const conversationId = this.getNodeParameter(
              'conversationId',
              itemIndex,
            ) as string;
            data = await getConversationContext(credentials, conversationId);
            break;
          }
          case 'execution': {
            const executionProcessId = this.getNodeParameter(
              'executionProcessId',
              itemIndex,
            ) as string;
            data = await getExecutionContext(credentials, executionProcessId);
            break;
          }
          case 'approval': {
            const approvalId = this.getNodeParameter('approvalId', itemIndex) as string;
            data = await getApprovalContext(credentials, approvalId);
            break;
          }
          default:
            throw new Error(`Unsupported VK read resource '${resource}'`);
        }

        returnData.push({
          json: normalizeReadOutput(resource, data),
          pairedItem: inputItems[itemIndex] ? { item: itemIndex } : undefined,
        });
      } catch (error) {
        if (this.continueOnFail()) {
          returnData.push({
            json: {
              error: error instanceof Error ? error.message : 'Unknown VK read error',
            },
            pairedItem: inputItems[itemIndex] ? { item: itemIndex } : undefined,
          });
          continue;
        }

        throw new NodeOperationError(this.getNode(), error, { itemIndex });
      }
    }

    return [returnData];
  }
}
