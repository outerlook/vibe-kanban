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
  getApprovalContext,
  getConversationContext,
  getExecutionContext,
  listGitHubRepositories,
  getTaskContext,
  getTaskGroupContext,
} from './shared/api';
import { normalizeReadOutput } from './shared/output';
import type { VkApiCredentialValue, VkReadResource } from './shared/vk-contracts';

export class VibeKanbanRead implements INodeType {
  methods = {
    loadOptions: {
      async getAvailableProjects(
        this: ILoadOptionsFunctions,
      ): Promise<INodePropertyOptions[]> {
        const credentials =
          await this.getCredentials<VkApiCredentialValue>('vibeKanbanApi');
        const workflowId =
          (this.getCurrentNodeParameter('workflowId') as string | undefined)?.trim() ||
          '';
        const repositories = await listGitHubRepositories(credentials, {
          workflowId: workflowId || undefined,
        });
        const projectMap = new Map<string, string>();

        for (const repository of repositories) {
          for (const [index, projectId] of repository.projectIds.entries()) {
            const projectName = repository.projectNames[index];
            if (projectName) {
              projectMap.set(projectId, projectName);
            }
          }
        }

        return Array.from(projectMap.entries())
          .sort((left, right) => left[1].localeCompare(right[1]))
          .map(([value, name]) => ({ name, value }));
      },

      async getAvailableGitHubRepositories(
        this: ILoadOptionsFunctions,
      ): Promise<INodePropertyOptions[]> {
        const credentials =
          await this.getCredentials<VkApiCredentialValue>('vibeKanbanApi');
        const workflowId =
          (this.getCurrentNodeParameter('workflowId') as string | undefined)?.trim() ||
          '';
        const projectIdsRaw = this.getCurrentNodeParameter('projectIds');
        const projectIds = Array.isArray(projectIdsRaw)
          ? projectIdsRaw.map((entry) => String(entry))
          : [];
        const repositories = await listGitHubRepositories(credentials, {
          workflowId: workflowId || undefined,
          projectIds,
        });

        return repositories.map((repository) => ({
          name:
            repository.projectNames.length > 0
              ? `${repository.github_full_name} (${repository.projectNames.join(', ')})`
              : repository.github_full_name,
          value: repository.github_full_name,
        }));
      },
    },
  };

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
          { name: 'GitHub Repositories', value: 'githubRepositories' },
        ],
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
      {
        displayName: 'Workflow ID',
        name: 'workflowId',
        type: 'string',
        default: '',
        required: false,
        description:
          'Optional VK workflow ID. When set, only repos from projects associated with this workflow are returned.',
        displayOptions: {
          show: {
            resource: ['githubRepositories'],
          },
        },
      },
      {
        displayName: 'Projects',
        name: 'projectIds',
        type: 'multiOptions',
        default: [],
        required: false,
        typeOptions: {
          loadOptionsMethod: 'getAvailableProjects',
          loadOptionsDependsOn: ['workflowId'],
        },
        description:
          'Optional project filter. Leave empty to include all matching projects.',
        displayOptions: {
          show: {
            resource: ['githubRepositories'],
          },
        },
      },
      {
        displayName: 'Allowed Repositories',
        name: 'allowedRepoFullNames',
        type: 'multiOptions',
        default: [],
        required: false,
        typeOptions: {
          loadOptionsMethod: 'getAvailableGitHubRepositories',
          loadOptionsDependsOn: ['workflowId', 'projectIds'],
        },
        description:
          'Optional allow-list. Leave empty to include every matching repository.',
        displayOptions: {
          show: {
            resource: ['githubRepositories'],
          },
        },
      },
      {
        displayName: 'Ignored Repositories',
        name: 'ignoredRepoFullNames',
        type: 'multiOptions',
        default: [],
        required: false,
        typeOptions: {
          loadOptionsMethod: 'getAvailableGitHubRepositories',
          loadOptionsDependsOn: ['workflowId', 'projectIds'],
        },
        description:
          'Optional ignore-list applied after the workflow/project filters.',
        displayOptions: {
          show: {
            resource: ['githubRepositories'],
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
            const taskId = this.getNodeParameter('taskId', itemIndex) as string;
            data = await getTaskContext(credentials, taskId);
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
          case 'githubRepositories': {
            const workflowId = this.getNodeParameter(
              'workflowId',
              itemIndex,
              '',
            ) as string;
            const projectIds = this.getNodeParameter(
              'projectIds',
              itemIndex,
              [],
            ) as string[];
            const allowedRepoFullNames = this.getNodeParameter(
              'allowedRepoFullNames',
              itemIndex,
              [],
            ) as string[];
            const ignoredRepoFullNames = this.getNodeParameter(
              'ignoredRepoFullNames',
              itemIndex,
              [],
            ) as string[];

            const repositories = await listGitHubRepositories(credentials, {
              workflowId: workflowId.trim() || undefined,
              projectIds,
              allowedRepos: allowedRepoFullNames,
              ignoredRepos: ignoredRepoFullNames,
            });

            const baseJson = inputItems[itemIndex]?.json ?? {};
            for (const repository of repositories) {
              returnData.push({
                json: {
                  ...baseJson,
                  ...normalizeReadOutput(resource, repository),
                },
                pairedItem: inputItems[itemIndex]
                  ? { item: itemIndex }
                  : undefined,
              });
            }
            continue;
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
