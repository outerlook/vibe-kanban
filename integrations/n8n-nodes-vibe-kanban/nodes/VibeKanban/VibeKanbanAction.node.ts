import type {
  IExecuteFunctions,
  ILoadOptionsFunctions,
  INodeExecutionData,
  INodePropertyOptions,
  INodeType,
  INodeTypeDescription,
} from 'n8n-workflow';
import { NodeConnectionTypes, NodeOperationError } from 'n8n-workflow';

import {
  answerApproval,
  cancelGenerateAndMerge,
  cancelConversationFollowUp,
  cancelTaskFollowUp,
  createConversation,
  createFeedback,
  createReviewAttention,
  getExecutorProfiles,
  queueGenerateAndMerge,
  queueConversationFollowUp,
  queueTaskFollowUp,
  sendConversationMessage,
  startTaskExecution,
  startTaskFollowUp,
  stopExecutionProcess,
} from './shared/api';
import {
  buildExecutorProfileId,
  toExecutorOptions,
  toExecutorVariantOptions,
} from './shared/executor-profiles';
import { normalizeActionOutput } from './shared/output';
import type {
  VkActionResource,
  VkApiCredentialValue,
  VkApprovalResponse,
  VkCreateConversationRequest,
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

function parseJsonValue<T>(raw: string, label: string): T {
  try {
    return JSON.parse(raw) as T;
  } catch (error) {
    throw new Error(
      `${label} must be valid JSON: ${error instanceof Error ? error.message : 'parse failed'}`,
    );
  }
}

export class VibeKanbanAction implements INodeType {
  methods = {
    loadOptions: {
      async getAvailableExecutors(
        this: ILoadOptionsFunctions,
      ): Promise<INodePropertyOptions[]> {
        const credentials = await this.getCredentials<VkApiCredentialValue>(
          'vibeKanbanApi',
        );
        const profiles = await getExecutorProfiles(credentials);
        return toExecutorOptions(profiles);
      },

      async getAvailableExecutorVariants(
        this: ILoadOptionsFunctions,
      ): Promise<INodePropertyOptions[]> {
        const credentials = await this.getCredentials<VkApiCredentialValue>(
          'vibeKanbanApi',
        );
        const profiles = await getExecutorProfiles(credentials);
        const executor = (this.getCurrentNodeParameter('executor') as string) || '';
        return toExecutorVariantOptions(profiles, executor);
      },
    },
  };

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
        default: 'startTaskExecution',
        displayOptions: { show: { resource: ['task'] } },
        options: [{ name: 'Start Task Execution', value: 'startTaskExecution' }],
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
        default: 'createConversation',
        displayOptions: { show: { resource: ['conversation'] } },
        options: [
          { name: 'Create Conversation', value: 'createConversation' },
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
        displayName: 'Project ID',
        name: 'projectId',
        type: 'string',
        default: '',
        required: true,
        displayOptions: {
          show: {
            resource: ['conversation'],
            operation: ['createConversation'],
          },
        },
      },
      {
        displayName: 'Workspace Strategy',
        name: 'workspaceStrategy',
        type: 'options',
        default: 'latest_or_create',
        displayOptions: {
          show: {
            resource: ['task'],
            operation: ['startTaskExecution'],
          },
        },
        options: [
          { name: 'Latest Or Create', value: 'latest_or_create' },
          { name: 'Create New', value: 'create_new' },
        ],
      },
      {
        displayName: 'Executor Strategy',
        name: 'executorStrategy',
        type: 'options',
        default: 'default',
        displayOptions: {
          show: {
            resource: ['task'],
            operation: ['startTaskExecution'],
          },
        },
        options: [
          { name: 'VK Default', value: 'default' },
          { name: 'Latest Or Default', value: 'latest_or_default' },
          { name: 'Explicit', value: 'explicit' },
        ],
      },
      {
        displayName: 'Executor',
        name: 'executor',
        type: 'options',
        default: '',
        required: false,
        typeOptions: {
          loadOptionsMethod: 'getAvailableExecutors',
        },
        description:
          'Choose the VK executor. For task execution this is only used when Executor Strategy is Explicit. Leave empty on Create Conversation to use the VK default executor.',
        displayOptions: {
          show: {
            resource: ['task', 'conversation'],
            operation: ['startTaskExecution', 'createConversation'],
          },
        },
      },
      {
        displayName: 'Executor Variant',
        name: 'executorVariant',
        type: 'options',
        default: '',
        required: false,
        typeOptions: {
          loadOptionsMethod: 'getAvailableExecutorVariants',
          loadOptionsDependsOn: ['executor'],
        },
        description: 'Leave empty to use the default variant of the selected executor.',
        displayOptions: {
          show: {
            resource: ['task', 'conversation'],
            operation: ['startTaskExecution', 'createConversation'],
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
            operation: ['startTaskExecution'],
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
            operation: ['startTaskExecution'],
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
        displayOptions: {
          show: {
            resource: ['conversation'],
            operation: ['sendMessage', 'queueFollowUp', 'cancelQueuedFollowUp'],
          },
        },
      },
      {
        displayName: 'Title',
        name: 'title',
        type: 'string',
        default: '',
        required: true,
        displayOptions: {
          show: {
            resource: ['conversation'],
            operation: ['createConversation'],
          },
        },
      },
      {
        displayName: 'Initial Message',
        name: 'initialMessage',
        type: 'string',
        typeOptions: { rows: 4 },
        default: '',
        required: true,
        displayOptions: {
          show: {
            resource: ['conversation'],
            operation: ['createConversation'],
          },
        },
      },
      {
        displayName: 'Worktree Path',
        name: 'worktreePath',
        type: 'string',
        default: '',
        required: false,
        displayOptions: {
          show: {
            resource: ['conversation'],
            operation: ['createConversation'],
          },
        },
      },
      {
        displayName: 'Worktree Branch',
        name: 'worktreeBranch',
        type: 'string',
        default: '',
        required: false,
        displayOptions: {
          show: {
            resource: ['conversation'],
            operation: ['createConversation'],
          },
        },
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
          case 'task': {
            const taskId = this.getNodeParameter('taskId', itemIndex) as string;
            const workspaceStrategy = this.getNodeParameter(
              'workspaceStrategy',
              itemIndex,
            ) as string;
            const executorStrategy = this.getNodeParameter(
              'executorStrategy',
              itemIndex,
            ) as string;
            const executor = this.getNodeParameter(
              'executor',
              itemIndex,
              '',
            ) as string;
            const executorVariant = this.getNodeParameter(
              'executorVariant',
              itemIndex,
              '',
            ) as string;
            const repoSelection = this.getNodeParameter('repoSelection', itemIndex) as string;
            const executorProfile = buildExecutorProfileId(
              executor,
              executorVariant,
            );
            let executorStrategyBody:
              | { executor_selection: 'default' }
              | { executor_selection: 'latest_or_default' }
              | {
                  executor_selection: 'explicit';
                  executor_profile_id: NonNullable<typeof executorProfile>;
                };

            if (executorStrategy === 'explicit') {
              if (!executorProfile) {
                throw new Error('Executor is required when Executor Strategy is Explicit');
              }
              executorStrategyBody = {
                executor_selection: 'explicit',
                executor_profile_id: executorProfile,
              };
            } else if (executorStrategy === 'latest_or_default') {
              executorStrategyBody = { executor_selection: 'latest_or_default' };
            } else {
              executorStrategyBody = { executor_selection: 'default' };
            }

            const command = {
              task_id: taskId,
              workspace_strategy: workspaceStrategy,
              executor_strategy: executorStrategyBody,
              repo_selection:
                repoSelection === 'explicit'
                  ? {
                      repo_selection: 'explicit',
                      repos: parseJsonValue<{ repo_id: string; target_branch: string }[]>(
                        this.getNodeParameter('reposJson', itemIndex) as string,
                        'Repos JSON',
                      ),
                    }
                  : {
                      repo_selection: 'task_group_default',
                    },
            };

            const data = await startTaskExecution(credentials, command as never);
            output = normalizeActionOutput({
              resource,
              operation,
              identifiers: { taskId },
              data,
            });
            break;
          }
          case 'workspace': {
            const workspaceId = this.getNodeParameter('workspaceId', itemIndex) as string;

            if (operation === 'queueGenerateAndMerge') {
              const repoId = this.getNodeParameter('repoId', itemIndex) as string;
              const data = await queueGenerateAndMerge(credentials, workspaceId, {
                repo_id: repoId,
              });
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { workspaceId, repoId },
                data,
              });
            } else {
              await cancelGenerateAndMerge(credentials, workspaceId);
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { workspaceId },
              });
            }
            break;
          }
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
            if (operation === 'createConversation') {
              const projectId = this.getNodeParameter('projectId', itemIndex) as string;
              const title = this.getNodeParameter('title', itemIndex) as string;
              const initialMessage = this.getNodeParameter(
                'initialMessage',
                itemIndex,
              ) as string;
              const executor = this.getNodeParameter(
                'executor',
                itemIndex,
                '',
              ) as string;
              const executorVariant = this.getNodeParameter(
                'executorVariant',
                itemIndex,
                '',
              ) as string;
              const worktreePath = this.getNodeParameter(
                'worktreePath',
                itemIndex,
                '',
              ) as string;
              const worktreeBranch = this.getNodeParameter(
                'worktreeBranch',
                itemIndex,
                '',
              ) as string;
              const body: VkCreateConversationRequest = {
                title,
                initial_message: initialMessage,
                executor_profile_id: buildExecutorProfileId(
                  executor,
                  executorVariant,
                ),
                worktree_path: worktreePath || null,
                worktree_branch: worktreeBranch || null,
              };
              const data = await createConversation(credentials, projectId, body);
              output = normalizeActionOutput({
                resource,
                operation,
                identifiers: { projectId },
                data,
              });
            } else {
              const conversationId = this.getNodeParameter(
                'conversationId',
                itemIndex,
              ) as string;
              if (operation === 'sendMessage') {
                const content = this.getNodeParameter('content', itemIndex) as string;
                const variant = this.getNodeParameter(
                  'variant',
                  itemIndex,
                  '',
                ) as string;
                const data = await sendConversationMessage(
                  credentials,
                  conversationId,
                  {
                    content,
                    variant: variant || undefined,
                  },
                );
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
          case 'feedback': {
            const executionProcessId = this.getNodeParameter(
              'executionProcessId',
              itemIndex,
            ) as string;
            const taskId = this.getNodeParameter('taskId', itemIndex) as string;
            const workspaceId = this.getNodeParameter('workspaceId', itemIndex) as string;
            const feedbackJson = this.getNodeParameter('feedbackJson', itemIndex, '') as string;
            const feedback = feedbackJson.trim()
              ? JSON.stringify(parseJsonValue<unknown>(feedbackJson, 'Feedback JSON'))
              : undefined;

            const data = await createFeedback(credentials, {
              execution_process_id: executionProcessId,
              task_id: taskId,
              workspace_id: workspaceId,
              feedback_json: feedback ?? null,
            });
            output = normalizeActionOutput({
              resource,
              operation,
              identifiers: { executionProcessId, taskId, workspaceId },
              data,
            });
            break;
          }
          case 'reviewAttention': {
            const executionProcessId = this.getNodeParameter(
              'executionProcessId',
              itemIndex,
            ) as string;
            const taskId = this.getNodeParameter('taskId', itemIndex) as string;
            const workspaceId = this.getNodeParameter('workspaceId', itemIndex) as string;
            const needsAttention = this.getNodeParameter(
              'needsAttention',
              itemIndex,
            ) as boolean;
            const reasoning = this.getNodeParameter('reasoning', itemIndex, '') as string;

            const data = await createReviewAttention(credentials, {
              execution_process_id: executionProcessId,
              task_id: taskId,
              workspace_id: workspaceId,
              needs_attention: needsAttention,
              reasoning: reasoning || null,
            });
            output = normalizeActionOutput({
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
