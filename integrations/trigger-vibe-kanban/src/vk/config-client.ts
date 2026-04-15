import { VkHttpClient } from './http-client';
import type {
  ProjectGitHubRepository,
  ProjectWithTaskCounts,
  VkApiConfig,
  WorkflowAssociation,
} from './types';

export type SelectedProject = ProjectWithTaskCounts & {
  workflowAssociation: WorkflowAssociation | null;
};

export type SelectedGitHubRepository = ProjectGitHubRepository & {
  projectIds: string[];
  projectNames: string[];
};

export class VkConfigClient extends VkHttpClient {
  constructor(config: VkApiConfig) {
    super(config);
  }

  async getProjects(): Promise<ProjectWithTaskCounts[]> {
    return this.request('/projects');
  }

  // Workflow association stays in config lookups so runtime checkpoints remain Trigger-owned.
  async getProjectWorkflowAssociation(
    projectId: string,
  ): Promise<WorkflowAssociation | null> {
    return this.request(`/projects/${projectId}/workflow-association`);
  }

  async getProjectGitHubRepositories(
    projectId: string,
  ): Promise<ProjectGitHubRepository[]> {
    return this.request(`/projects/${projectId}/github-repositories`);
  }

  async listProjects(options?: {
    workflowId?: string;
    projectIds?: string[];
  }): Promise<SelectedProject[]> {
    const workflowId = options?.workflowId?.trim() || '';
    const selectedProjectIds = new Set(
      (options?.projectIds ?? []).map((entry) => entry.trim()).filter(Boolean),
    );

    const projects = await this.getProjects();
    const selectedProjects: SelectedProject[] = [];

    for (const project of projects) {
      const projectId = String(project.id);
      if (selectedProjectIds.size > 0 && !selectedProjectIds.has(projectId)) {
        continue;
      }

      const workflowAssociation = workflowId
        ? await this.getProjectWorkflowAssociation(projectId)
        : null;

      if (workflowId && workflowAssociation?.workflow_id !== workflowId) {
        continue;
      }

      selectedProjects.push({
        ...project,
        workflowAssociation,
      });
    }

    return selectedProjects.sort((left, right) => left.name.localeCompare(right.name));
  }

  async listGitHubRepositories(options?: {
    workflowId?: string;
    projectIds?: string[];
    allowedRepos?: string[];
    ignoredRepos?: string[];
  }): Promise<SelectedGitHubRepository[]> {
    const allowedRepos = new Set(
      (options?.allowedRepos ?? []).map((entry) => entry.trim().toLowerCase()).filter(Boolean),
    );
    const ignoredRepos = new Set(
      (options?.ignoredRepos ?? []).map((entry) => entry.trim().toLowerCase()).filter(Boolean),
    );

    const selectedRepoMap = new Map<string, SelectedGitHubRepository>();
    const projects = await this.listProjects({
      ...(options?.workflowId ? { workflowId: options.workflowId } : {}),
      ...(options?.projectIds ? { projectIds: options.projectIds } : {}),
    });

    for (const project of projects) {
      const projectId = String(project.id);
      const repositories = await this.getProjectGitHubRepositories(projectId);

      for (const repository of repositories) {
        const fullName = repository.github_full_name.trim().toLowerCase();
        if (!fullName) {
          continue;
        }

        if (allowedRepos.size > 0 && !allowedRepos.has(fullName)) {
          continue;
        }

        if (ignoredRepos.has(fullName)) {
          continue;
        }

        const existing = selectedRepoMap.get(fullName);
        if (existing) {
          if (!existing.projectIds.includes(projectId)) {
            existing.projectIds.push(projectId);
          }
          if (!existing.projectNames.includes(project.name)) {
            existing.projectNames.push(project.name);
          }
          continue;
        }

        selectedRepoMap.set(fullName, {
          ...repository,
          projectIds: [projectId],
          projectNames: [project.name],
        });
      }
    }

    return [...selectedRepoMap.values()].sort((left, right) =>
      left.github_full_name.localeCompare(right.github_full_name),
    );
  }
}
