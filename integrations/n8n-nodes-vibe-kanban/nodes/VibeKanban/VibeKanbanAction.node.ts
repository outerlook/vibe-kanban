import type {
  IExecuteFunctions,
  INodeExecutionData,
  INodeType,
  INodeTypeDescription,
} from 'n8n-workflow';
import { NodeConnectionTypes, NodeOperationError } from 'n8n-workflow';

import {
  answerApproval,
  cancelConversationFollowUp,
  cancelTaskFollowUp,
  queueConversationFollowUp,
  queueTaskFollowUp,
  sendConversationMessage,
  startTaskFollowUp,
  stopExecutionProcess,
} from './shared/api';
import { normalizeActionOutput } from './shared/output';
import type {
  VkActionResource,
  VkApiCredentialValue,
  VkApprovalResponse,
  VkQuestionAnswer,
} from './shared/vk-contracts';

function parseAnswersJson(raw: string): VkQuestionAnswer[] {
  if (!raw.trim()) {
    return [];
  }

  const parsed = JSON.parse(raw);
  if (!Array.isArray(parsed)) {
    throw new Error('Answers JSON must be an array of QuestionAnswer objects');
  }
  return parsed as VkQuestionAnswer[];
}

export class VibeKanbanAction implements INodeType {
  description: INodeTypeDescription = {
    displayName: 'Vibe Kanban Action',
    name: 'vibeKanbanAction',
    icon: 'file:vibeKanban.svg',
    group: ['transform'],
    version: 1,
    description: 'Invoke VK orchestration command surfaces without generic HTTP glue',
    defaults: {
      name: 'Vibe Kanban Action',
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
        default: 'taskSession',
        options: [
          { name: 'Task Session', value: 'taskSession' },
          { name: 'Conversation', value: 'conversation' },
          { name: 'Approval', value: 'approval' },
          { name: 'Execution', value: 'execution' },
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
            resource: ['approval', 'execution'],
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
        ) as VkActionResource;
        const operation = this.getNodeParameter('operation', itemIndex) as string;

        let output;
        switch (resource) {
          case 'taskSession': {
            const sessionId = this.getNodeParameter('sessionId', itemIndex) as string;
            if (operation === 'startFollowUp') {
              const prompt = this.getNodeParameter('prompt', itemIndex) as string;
              const variant = this.getNodeParameter('variant', itemIndex, '') as string;
              const retryProcessId = this.getNodeParameter(
                'retryProcessId',
                itemIndex,
                '',
              ) as string;
              const forceWhenDirty = this.getNodeParameter(
                'forceWhenDirty',
                itemIndex,
              ) as boolean;
              const performGitReset = this.getNodeParameter(
                'performGitReset',
                itemIndex,
              ) as boolean;
              const data = await startTaskFollowUp(credentials, sessionId, {
                prompt,
                variant: variant || undefined,
                retry_process_id: retryProcessId || undefined,
                force_when_dirty: forceWhenDirty,
                perform_git_reset: performGitReset,
              });
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { sessionId },
                data,
              });
            } else if (operation === 'queueFollowUp') {
              const message = this.getNodeParameter('message', itemIndex) as string;
              const variant = this.getNodeParameter('variant', itemIndex, '') as string;
              const data = await queueTaskFollowUp(credentials, sessionId, {
                message,
                variant: variant || undefined,
              });
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { sessionId },
                data,
              });
            } else {
              const data = await cancelTaskFollowUp(credentials, sessionId);
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { sessionId },
                data,
              });
            }
            break;
          }
          case 'conversation': {
            const conversationId = this.getNodeParameter(
              'conversationId',
              itemIndex,
            ) as string;
            if (operation === 'sendMessage') {
              const content = this.getNodeParameter('content', itemIndex) as string;
              const variant = this.getNodeParameter('variant', itemIndex, '') as string;
              const data = await sendConversationMessage(credentials, conversationId, {
                content,
                variant: variant || undefined,
              });
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { conversationId },
                data,
              });
            } else if (operation === 'queueFollowUp') {
              const message = this.getNodeParameter('message', itemIndex) as string;
              const variant = this.getNodeParameter('variant', itemIndex, '') as string;
              const data = await queueConversationFollowUp(credentials, conversationId, {
                message,
                variant: variant || undefined,
              });
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { conversationId },
                data,
              });
            } else {
              const data = await cancelConversationFollowUp(
                credentials,
                conversationId,
              );
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { conversationId },
                data,
              });
            }
            break;
          }
          case 'approval': {
            const approvalId = this.getNodeParameter('approvalId', itemIndex) as string;
            const executionProcessId = this.getNodeParameter(
              'executionProcessId',
              itemIndex,
            ) as string;
            const approvalResponse = this.getNodeParameter(
              'approvalResponse',
              itemIndex,
            ) as string;

            let body: VkApprovalResponse;
            if (approvalResponse === 'approved') {
              body = {
                execution_process_id: executionProcessId,
                status: { status: 'approved' },
              };
            } else if (approvalResponse === 'denied') {
              const reason = this.getNodeParameter('denyReason', itemIndex, '') as string;
              body = {
                execution_process_id: executionProcessId,
                status: { status: 'denied', reason: reason || undefined },
              };
            } else if (approvalResponse === 'answered') {
              const answersJson = this.getNodeParameter('answersJson', itemIndex) as string;
              const answers = parseAnswersJson(answersJson);
              body = {
                execution_process_id: executionProcessId,
                status: { status: 'answered', answers },
                answers,
              };
            } else {
              body = {
                execution_process_id: executionProcessId,
                status: { status: 'timed_out' },
              };
            }

            const data = await answerApproval(credentials, approvalId, body);
            output = normalizeActionOutput({
              resource,
              operation,
              identifiers: { approvalId, executionProcessId },
              data,
            });
            break;
          }
          case 'execution': {
            const executionProcessId = this.getNodeParameter(
              'executionProcessId',
              itemIndex,
            ) as string;
            await stopExecutionProcess(credentials, executionProcessId);
            output = normalizeActionOutput({
              resource,
              operation,
              identifiers: { executionProcessId },
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
      } catch (error) {
        if (this.continueOnFail()) {
          returnData.push({
            json: {
              error: error instanceof Error ? error.message : 'Unknown VK action error',
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
