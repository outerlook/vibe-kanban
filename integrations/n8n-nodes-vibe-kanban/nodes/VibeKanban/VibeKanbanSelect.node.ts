import type {
  IExecuteFunctions,
  ILoadOptionsFunctions,
  INodeExecutionData,
  INodePropertyOptions,
  INodeType,
  INodeTypeDescription,
} from 'n8n-workflow';
import { NodeConnectionTypes, NodeOperationError } from 'n8n-workflow';

import { listGitHubRepositories, listProjects } from './shared/api';
import { normalizeSelectOutput } from './shared/output';
import type {
  VkApiCredentialValue,
  VkSelectResource,
  VkSelectedGitHubRepository,
  VkSelectedProject,
} from './shared/vk-contracts';

export class VibeKanbanSelect implements INodeType {
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
        const projects = await listProjects(credentials, {
          workflowId: workflowId || undefined,
        });

        return projects.map((project) => ({
          name: project.name,
          value: project.id,
        }));
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
    displayName: 'Vibe Kanban Select',
    name: 'vibeKanbanSelect',
    icon: 'file:vibeKanban.svg',
    group: ['transform'],
    version: 1,
    description:
      'Select VK-backed entities through dynamic pickers and emit normalized records',
    defaults: {
      name: 'Vibe Kanban Select',
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
        default: 'projects',
        options: [
          { name: 'Projects', value: 'projects' },
          { name: 'GitHub Repositories', value: 'githubRepositories' },
        ],
      },
      {
        displayName: 'Workflow ID',
        name: 'workflowId',
        type: 'string',
        default: '',
        required: false,
        description:
          'Optional VK workflow ID. When set, only projects associated with this workflow are included.',
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
        ) as VkSelectResource;
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
        const baseJson = inputItems[itemIndex]?.json ?? {};

        let selections: Array<VkSelectedProject | VkSelectedGitHubRepository>;
        switch (resource) {
          case 'projects':
            selections = await listProjects(credentials, {
              workflowId: workflowId.trim() || undefined,
              projectIds,
            });
            break;
          case 'githubRepositories': {
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

            selections = await listGitHubRepositories(credentials, {
              workflowId: workflowId.trim() || undefined,
              projectIds,
              allowedRepos: allowedRepoFullNames,
              ignoredRepos: ignoredRepoFullNames,
            });
            break;
          }
          default:
            throw new Error(`Unsupported VK select resource '${resource}'`);
        }

        for (const selection of selections) {
          returnData.push({
            json: {
              ...baseJson,
              ...normalizeSelectOutput(resource, selection),
            },
            pairedItem: inputItems[itemIndex] ? { item: itemIndex } : undefined,
          });
        }
      } catch (error) {
        if (this.continueOnFail()) {
          returnData.push({
            json: {
              error: error instanceof Error ? error.message : 'Unknown VK select error',
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
