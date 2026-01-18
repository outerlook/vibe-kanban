// Import all necessary types from shared types

import { isTauriEnvironment, getServerUrl } from './tauriApi';
import {
  AccountInfo,
  ApprovalStatus,
  ApiResponse,
  BranchAncestorStatus,
  Config,
  CreateFollowUpAttempt,
  EditorType,
  CreateGitHubPrRequest,
  CreateTask,
  CreateAndStartTaskRequest,
  CreateTaskAttemptBody,
  CreateTag,
  CreateTaskGroup,
  DirectoryListResponse,
  DirectoryEntry,
  ExecutionProcess,
  ExecutionProcessRepoState,
  GanttTask,
  GitBranch,
  GitHubImportResponse,
  GitHubSettingsStatus,
  Project,
  ProjectRepo,
  Repo,
  RepoWithTargetBranch,
  CreateProject,
  CreateProjectRepo,
  UpdateProjectRepo,
  SearchResult,
  ShareTaskResponse,
  Task,
  TaskDependency,
  TaskGroup,
  TaskGroupWithStats,
  TaskRelationships,
  Tag,
  TagSearchParams,
  TaskWithAttemptStatus,
  TaskStatus,
  UpdateProject,
  UpdateTask,
  UpdateTag,
  UpdateTaskGroup,
  UserSystemInfo,
  McpServerQuery,
  UpdateMcpServersBody,
  GetMcpServerResponse,
  ImageResponse,
  GitOperationError,
  ApprovalResponse,
  RebaseTaskAttemptRequest,
  ChangeTargetBranchRequest,
  ChangeTargetBranchResponse,
  RenameBranchRequest,
  RenameBranchResponse,
  CheckEditorAvailabilityResponse,
  CreateCustomEditorRequest,
  UpdateCustomEditorRequest,
  CustomEditorResponse,
  ListCustomEditorsResponse,
  AvailabilityInfo,
  BaseCodingAgent,
  RunAgentSetupRequest,
  RunAgentSetupResponse,
  GhCliSetupError,
  RunScriptError,
  StatusResponse,
  ListOrganizationsResponse,
  OrganizationMemberWithProfile,
  ListMembersResponse,
  RemoteProjectMembersResponse,
  CreateOrganizationRequest,
  CreateOrganizationResponse,
  CreateInvitationRequest,
  CreateInvitationResponse,
  RevokeInvitationRequest,
  UpdateMemberRoleRequest,
  CreateRemoteProjectRequest,
  LinkToExistingRequest,
  UpdateMemberRoleResponse,
  Invitation,
  RemoteProject,
  ListInvitationsResponse,
  OpenEditorResponse,
  OpenEditorRequest,
  CreatePrError,
  Scratch,
  ScratchType,
  CreateScratch,
  UpdateScratch,
  PushError,
  TokenResponse,
  CurrentUserResponse,
  SharedTaskResponse,
  SharedTaskDetails,
  QueueStatus,
  QueueMergeRequest,
  QueueMergeError,
  MergeQueue,
  MergeQueueCountResponse,
  FollowUpResult,
  PrCommentsResponse,
  NormalizedEntry,
  MergeTaskAttemptRequest,
  PushTaskAttemptRequest,
  GenerateCommitMessageRequest,
  GenerateCommitMessageResponse,
  RepoBranchStatus,
  AbortConflictsRequest,
  Session,
  Workspace,
  AvailableSoundsResponse,
  Notification,
  NotificationStats,
  UpdateNotification,
  ConversationSession,
  ConversationSessionStatus,
  ConversationMessage,
  ConversationMessagesPage,
  ConversationWithMessages,
  SendMessageResponse,
  ProjectPrsResponse,
  RepoPrs,
  PrWithComments,
  FeedbackResponse,
} from 'shared/types';
import type { WorkspaceWithSession } from '@/types/attempt';
import { createWorkspaceWithSession } from '@/types/attempt';

export class ApiError<E = unknown> extends Error {
  public status?: number;
  public error_data?: E;

  constructor(
    message: string,
    public statusCode?: number,
    public response?: Response,
    error_data?: E
  ) {
    super(message);
    this.name = 'ApiError';
    this.status = statusCode;
    this.error_data = error_data;
  }
}

export interface NormalizedEntriesPageEntry {
  entry_index: number;
  entry: NormalizedEntry;
}

export interface ExecutionProcessNormalizedEntriesPage {
  entries: NormalizedEntriesPageEntry[];
  next_before_index: number | null;
  has_more: boolean;
}

export interface PaginatedTasksResponse {
  tasks: TaskWithAttemptStatus[];
  total: number;
  hasMore: boolean;
}

export interface PaginatedGanttResponse {
  tasks: GanttTask[];
  total: number;
  hasMore: boolean;
}

export type TaskDependencyTreeNode = {
  task: Task;
  dependencies: TaskDependencyTreeNode[];
};

export type DependencyDirection = 'blocked_by' | 'blocking';

// Cached base URL for API requests (resolved once on first call)
let apiBaseUrl: string | null = null;

async function getApiBaseUrl(): Promise<string> {
  if (apiBaseUrl !== null) return apiBaseUrl;
  const url = isTauriEnvironment() ? await getServerUrl() : '';
  apiBaseUrl = url;
  return url;
}

/**
 * Get the base URL synchronously. Returns cached value or empty string.
 * Use this for synchronous URL construction (e.g., image URLs).
 * Call initApiBaseUrl() during app startup to ensure this is populated.
 */
export function getApiBaseUrlSync(): string {
  return apiBaseUrl ?? '';
}

/**
 * Initialize the API base URL. Call this during app startup.
 */
export async function initApiBaseUrl(): Promise<string> {
  return getApiBaseUrl();
}

/**
 * Reset the cached API base URL. Call this after changing server mode
 * so the next API request will re-fetch the URL from Tauri.
 */
export function resetApiBaseUrl(): void {
  apiBaseUrl = null;
}

/**
 * Refresh the API base URL. Call this after switching server modes.
 * Clears the cache and re-initializes from the current server URL.
 */
export async function refreshApiBaseUrl(): Promise<string> {
  resetApiBaseUrl();
  return getApiBaseUrl();
}

// Re-export PR types from shared for convenience
export type { ProjectPrsResponse, RepoPrs, PrWithComments };

const makeRequest = async (url: string, options: RequestInit = {}) => {
  const baseUrl = await getApiBaseUrl();
  const headers = new Headers(options.headers ?? {});
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }

  return fetch(`${baseUrl}${url}`, {
    ...options,
    headers,
  });
};

export type Ok<T> = { success: true; data: T };
export type Err<E> = { success: false; error: E | undefined; message?: string };

// Result type for endpoints that need typed errors
export type Result<T, E> = Ok<T> | Err<E>;

// Special handler for Result-returning endpoints
const handleApiResponseAsResult = async <T, E>(
  response: Response
): Promise<Result<T, E>> => {
  if (!response.ok) {
    // HTTP error - no structured error data
    let errorMessage = `Request failed with status ${response.status}`;

    try {
      const errorData = await response.json();
      if (errorData.message) {
        errorMessage = errorData.message;
      }
    } catch {
      errorMessage = response.statusText || errorMessage;
    }

    return {
      success: false,
      error: undefined,
      message: errorMessage,
    };
  }

  const result: ApiResponse<T, E> = await response.json();

  if (!result.success) {
    return {
      success: false,
      error: result.error_data || undefined,
      message: result.message || undefined,
    };
  }

  return { success: true, data: result.data as T };
};

export const handleApiResponse = async <T, E = T>(
  response: Response
): Promise<T> => {
  if (!response.ok) {
    let errorMessage = `Request failed with status ${response.status}`;

    try {
      const errorData = await response.json();
      if (errorData.message) {
        errorMessage = errorData.message;
      }
    } catch {
      // Fallback to status text if JSON parsing fails
      errorMessage = response.statusText || errorMessage;
    }

    console.error('[API Error]', {
      message: errorMessage,
      status: response.status,
      response,
      endpoint: response.url,
      timestamp: new Date().toISOString(),
    });
    throw new ApiError<E>(errorMessage, response.status, response);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const result: ApiResponse<T, E> = await response.json();

  if (!result.success) {
    // Check for error_data first (structured errors), then fall back to message
    if (result.error_data) {
      console.error('[API Error with data]', {
        error_data: result.error_data,
        message: result.message,
        status: response.status,
        response,
        endpoint: response.url,
        timestamp: new Date().toISOString(),
      });
      // Throw a properly typed error with the error data
      throw new ApiError<E>(
        result.message || 'API request failed',
        response.status,
        response,
        result.error_data
      );
    }

    console.error('[API Error]', {
      message: result.message || 'API request failed',
      status: response.status,
      response,
      endpoint: response.url,
      timestamp: new Date().toISOString(),
    });
    throw new ApiError<E>(
      result.message || 'API request failed',
      response.status,
      response
    );
  }

  return result.data as T;
};

// Project Management APIs
export const projectsApi = {
  create: async (data: CreateProject): Promise<Project> => {
    const response = await makeRequest('/api/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Project>(response);
  },

  update: async (id: string, data: UpdateProject): Promise<Project> => {
    const response = await makeRequest(`/api/projects/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Project>(response);
  },

  getRemoteMembers: async (
    projectId: string
  ): Promise<RemoteProjectMembersResponse> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/remote/members`
    );
    return handleApiResponse<RemoteProjectMembersResponse>(response);
  },

  delete: async (id: string): Promise<void> => {
    const response = await makeRequest(`/api/projects/${id}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  openEditor: async (
    id: string,
    data: OpenEditorRequest
  ): Promise<OpenEditorResponse> => {
    const response = await makeRequest(`/api/projects/${id}/open-editor`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<OpenEditorResponse>(response);
  },

  searchFiles: async (
    id: string,
    query: string,
    mode?: string,
    options?: RequestInit
  ): Promise<SearchResult[]> => {
    const modeParam = mode ? `&mode=${encodeURIComponent(mode)}` : '';
    const response = await makeRequest(
      `/api/projects/${id}/search?q=${encodeURIComponent(query)}${modeParam}`,
      options
    );
    return handleApiResponse<SearchResult[]>(response);
  },

  linkToExisting: async (
    localProjectId: string,
    data: LinkToExistingRequest
  ): Promise<Project> => {
    const response = await makeRequest(`/api/projects/${localProjectId}/link`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Project>(response);
  },

  createAndLink: async (
    localProjectId: string,
    data: CreateRemoteProjectRequest
  ): Promise<Project> => {
    const response = await makeRequest(
      `/api/projects/${localProjectId}/link/create`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<Project>(response);
  },

  unlink: async (projectId: string): Promise<Project> => {
    const response = await makeRequest(`/api/projects/${projectId}/link`, {
      method: 'DELETE',
    });
    return handleApiResponse<Project>(response);
  },

  getRepositories: async (projectId: string): Promise<Repo[]> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/repositories`
    );
    return handleApiResponse<Repo[]>(response);
  },

  addRepository: async (
    projectId: string,
    data: CreateProjectRepo
  ): Promise<Repo> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/repositories`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<Repo>(response);
  },

  deleteRepository: async (
    projectId: string,
    repoId: string
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/repositories/${repoId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  getRepository: async (
    projectId: string,
    repoId: string
  ): Promise<ProjectRepo> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/repositories/${repoId}`
    );
    return handleApiResponse<ProjectRepo>(response);
  },

  updateRepository: async (
    projectId: string,
    repoId: string,
    data: UpdateProjectRepo
  ): Promise<ProjectRepo> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/repositories/${repoId}`,
      {
        method: 'PUT',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<ProjectRepo>(response);
  },

  getPullRequests: async (projectId: string): Promise<ProjectPrsResponse> => {
    const response = await makeRequest(`/api/projects/${projectId}/prs`);
    return handleApiResponse<ProjectPrsResponse>(response);
  },

  getMergeQueueCount: async (
    projectId: string
  ): Promise<MergeQueueCountResponse> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/merge-queue-count`
    );
    return handleApiResponse<MergeQueueCountResponse>(response);
  },

  getWorkspaces: async (projectId: string): Promise<Workspace[]> => {
    const response = await makeRequest(`/api/projects/${projectId}/workspaces`);
    return handleApiResponse<Workspace[]>(response);
  },
};

// Gantt API
export const ganttApi = {
  list: async (
    projectId: string,
    params?: {
      offset?: number;
      limit?: number;
    }
  ): Promise<PaginatedGanttResponse> => {
    const search = new URLSearchParams();
    if (params?.offset !== undefined) {
      search.set('offset', params.offset.toString());
    }
    if (params?.limit !== undefined) {
      search.set('limit', params.limit.toString());
    }
    const queryString = search.toString();
    const url = `/api/projects/${projectId}/gantt${queryString ? `?${queryString}` : ''}`;
    const response = await makeRequest(url);
    return handleApiResponse<PaginatedGanttResponse>(response);
  },
};

// Task Management APIs
export const tasksApi = {
  list: async (
    projectId: string,
    params?: {
      offset?: number;
      limit?: number;
      status?: TaskStatus;
      order_by?: 'created_at_asc' | 'created_at_desc' | 'updated_at_asc' | 'updated_at_desc';
    }
  ): Promise<PaginatedTasksResponse> => {
    const search = new URLSearchParams({ project_id: projectId });
    if (params?.offset !== undefined) {
      search.set('offset', params.offset.toString());
    }
    if (params?.limit !== undefined) {
      search.set('limit', params.limit.toString());
    }
    if (params?.status) {
      search.set('status', params.status);
    }
    if (params?.order_by) {
      search.set('order_by', params.order_by);
    }

    const response = await makeRequest(`/api/tasks?${search.toString()}`);
    return handleApiResponse<PaginatedTasksResponse>(response);
  },
  getById: async (taskId: string): Promise<Task> => {
    const response = await makeRequest(`/api/tasks/${taskId}`);
    return handleApiResponse<Task>(response);
  },

  create: async (data: CreateTask): Promise<Task> => {
    const response = await makeRequest(`/api/tasks`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Task>(response);
  },

  createAndStart: async (
    data: CreateAndStartTaskRequest
  ): Promise<TaskWithAttemptStatus> => {
    const response = await makeRequest(`/api/tasks/create-and-start`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<TaskWithAttemptStatus>(response);
  },

  update: async (taskId: string, data: UpdateTask): Promise<Task> => {
    const response = await makeRequest(`/api/tasks/${taskId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Task>(response);
  },

  delete: async (taskId: string): Promise<void> => {
    const response = await makeRequest(`/api/tasks/${taskId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  share: async (taskId: string): Promise<ShareTaskResponse> => {
    const response = await makeRequest(`/api/tasks/${taskId}/share`, {
      method: 'POST',
    });
    return handleApiResponse<ShareTaskResponse>(response);
  },

  reassign: async (
    sharedTaskId: string,
    data: { new_assignee_user_id: string | null }
  ): Promise<SharedTaskResponse> => {
    const payload = {
      new_assignee_user_id: data.new_assignee_user_id,
    };

    const response = await makeRequest(
      `/api/shared-tasks/${sharedTaskId}/assign`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );

    return handleApiResponse<SharedTaskResponse>(response);
  },

  unshare: async (sharedTaskId: string): Promise<void> => {
    const response = await makeRequest(`/api/shared-tasks/${sharedTaskId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  linkToLocal: async (data: SharedTaskDetails): Promise<Task | null> => {
    const response = await makeRequest(`/api/shared-tasks/link-to-local`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Task | null>(response);
  },
};

// Task Dependencies APIs
export const taskDependenciesApi = {
  getDependencies: async (
    taskId: string,
    direction?: DependencyDirection
  ): Promise<Task[]> => {
    const query = new URLSearchParams();
    if (direction) {
      query.set('direction', direction);
    }
    const suffix = query.toString();
    const response = await makeRequest(
      `/api/tasks/${taskId}/dependencies${suffix ? `?${suffix}` : ''}`
    );
    return handleApiResponse<Task[]>(response);
  },

  addDependency: async (
    taskId: string,
    dependsOnId: string
  ): Promise<TaskDependency> => {
    const response = await makeRequest(`/api/tasks/${taskId}/dependencies`, {
      method: 'POST',
      body: JSON.stringify({ depends_on_id: dependsOnId }),
    });
    return handleApiResponse<TaskDependency>(response);
  },

  removeDependency: async (
    taskId: string,
    dependsOnId: string
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/tasks/${taskId}/dependencies/${dependsOnId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  getDependencyTree: async (
    taskId: string,
    maxDepth?: number
  ): Promise<TaskDependencyTreeNode> => {
    const query = new URLSearchParams();
    if (maxDepth !== undefined) {
      query.set('max_depth', maxDepth.toString());
    }
    const suffix = query.toString();
    const response = await makeRequest(
      `/api/tasks/${taskId}/dependency-tree${suffix ? `?${suffix}` : ''}`
    );
    return handleApiResponse<TaskDependencyTreeNode>(response);
  },
};

// Task Groups API
export const taskGroupsApi = {
  getByProject: async (projectId: string): Promise<TaskGroup[]> => {
    const response = await makeRequest(
      `/api/task-groups?project_id=${encodeURIComponent(projectId)}`
    );
    return handleApiResponse<TaskGroup[]>(response);
  },

  getStatsForProject: async (projectId: string): Promise<TaskGroupWithStats[]> => {
    const response = await makeRequest(
      `/api/task-groups/stats?project_id=${encodeURIComponent(projectId)}`
    );
    return handleApiResponse<TaskGroupWithStats[]>(response);
  },

  getById: async (groupId: string): Promise<TaskGroup> => {
    const response = await makeRequest(`/api/task-groups/${groupId}`);
    return handleApiResponse<TaskGroup>(response);
  },

  create: async (data: CreateTaskGroup): Promise<TaskGroup> => {
    const response = await makeRequest('/api/task-groups', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<TaskGroup>(response);
  },

  update: async (groupId: string, data: UpdateTaskGroup): Promise<TaskGroup> => {
    const response = await makeRequest(`/api/task-groups/${groupId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<TaskGroup>(response);
  },

  delete: async (groupId: string): Promise<void> => {
    const response = await makeRequest(`/api/task-groups/${groupId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  assignTasks: async (groupId: string, taskIds: string[]): Promise<void> => {
    const response = await makeRequest(`/api/task-groups/${groupId}/assign`, {
      method: 'POST',
      body: JSON.stringify({ task_ids: taskIds }),
    });
    return handleApiResponse<void>(response);
  },

  merge: async (sourceGroupId: string, targetGroupId: string): Promise<TaskGroup> => {
    const response = await makeRequest(`/api/task-groups/${sourceGroupId}/merge`, {
      method: 'POST',
      body: JSON.stringify({ target_group_id: targetGroupId }),
    });
    return handleApiResponse<TaskGroup>(response);
  },

  getMergeQueueCount: async (groupId: string): Promise<MergeQueueCountResponse> => {
    const response = await makeRequest(
      `/api/task-groups/${groupId}/merge-queue-count`
    );
    return handleApiResponse<MergeQueueCountResponse>(response);
  },
};

// Sessions API
export const sessionsApi = {
  getByWorkspace: async (workspaceId: string): Promise<Session[]> => {
    const response = await makeRequest(
      `/api/sessions?workspace_id=${workspaceId}`
    );
    return handleApiResponse<Session[]>(response);
  },

  getById: async (sessionId: string): Promise<Session> => {
    const response = await makeRequest(`/api/sessions/${sessionId}`);
    return handleApiResponse<Session>(response);
  },

  create: async (data: {
    workspace_id: string;
    executor?: string;
  }): Promise<Session> => {
    const response = await makeRequest('/api/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Session>(response);
  },

  followUp: async (
    sessionId: string,
    data: CreateFollowUpAttempt
  ): Promise<FollowUpResult> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/follow-up`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<FollowUpResult>(response);
  },
};

// Task Attempts APIs
export const attemptsApi = {
  getChildren: async (attemptId: string): Promise<TaskRelationships> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/children`
    );
    return handleApiResponse<TaskRelationships>(response);
  },

  getAll: async (taskId: string): Promise<Workspace[]> => {
    const response = await makeRequest(`/api/task-attempts?task_id=${taskId}`);
    return handleApiResponse<Workspace[]>(response);
  },

  get: async (attemptId: string): Promise<Workspace> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}`);
    return handleApiResponse<Workspace>(response);
  },

  /** Get workspace with latest session */
  getWithSession: async (attemptId: string): Promise<WorkspaceWithSession> => {
    const [workspace, sessions] = await Promise.all([
      attemptsApi.get(attemptId),
      sessionsApi.getByWorkspace(attemptId),
    ]);
    return createWorkspaceWithSession(workspace, sessions[0]);
  },

  create: async (data: CreateTaskAttemptBody): Promise<Workspace> => {
    const response = await makeRequest(`/api/task-attempts`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Workspace>(response);
  },

  stop: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}/stop`, {
      method: 'POST',
    });
    return handleApiResponse<void>(response);
  },

  runAgentSetup: async (
    attemptId: string,
    data: RunAgentSetupRequest
  ): Promise<RunAgentSetupResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/run-agent-setup`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<RunAgentSetupResponse>(response);
  },

  openEditor: async (
    attemptId: string,
    data: OpenEditorRequest
  ): Promise<OpenEditorResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/open-editor`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<OpenEditorResponse>(response);
  },

  getBranchStatus: async (attemptId: string): Promise<RepoBranchStatus[]> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/branch-status`
    );
    return handleApiResponse<RepoBranchStatus[]>(response);
  },

  getRepos: async (attemptId: string): Promise<RepoWithTargetBranch[]> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}/repos`);
    return handleApiResponse<RepoWithTargetBranch[]>(response);
  },

  merge: async (
    attemptId: string,
    data: MergeTaskAttemptRequest
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/merge`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<void>(response);
  },

  generateCommitMessage: async (
    attemptId: string,
    data: GenerateCommitMessageRequest
  ): Promise<GenerateCommitMessageResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/generate-commit-message`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<GenerateCommitMessageResponse>(response);
  },

  push: async (
    attemptId: string,
    data: PushTaskAttemptRequest
  ): Promise<Result<void, PushError>> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}/push`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponseAsResult<void, PushError>(response);
  },

  forcePush: async (
    attemptId: string,
    data: PushTaskAttemptRequest
  ): Promise<Result<void, PushError>> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/push/force`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<void, PushError>(response);
  },

  rebase: async (
    attemptId: string,
    data: RebaseTaskAttemptRequest
  ): Promise<Result<void, GitOperationError>> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/rebase`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<void, GitOperationError>(response);
  },

  change_target_branch: async (
    attemptId: string,
    data: ChangeTargetBranchRequest
  ): Promise<ChangeTargetBranchResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/change-target-branch`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<ChangeTargetBranchResponse>(response);
  },

  renameBranch: async (
    attemptId: string,
    newBranchName: string
  ): Promise<RenameBranchResponse> => {
    const payload: RenameBranchRequest = {
      new_branch_name: newBranchName,
    };
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/rename-branch`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
    return handleApiResponse<RenameBranchResponse>(response);
  },

  abortConflicts: async (
    attemptId: string,
    data: AbortConflictsRequest
  ): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/conflicts/abort`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<void>(response);
  },

  createPR: async (
    attemptId: string,
    data: CreateGitHubPrRequest
  ): Promise<Result<string, CreatePrError>> => {
    const response = await makeRequest(`/api/task-attempts/${attemptId}/pr`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponseAsResult<string, CreatePrError>(response);
  },

  startDevServer: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/start-dev-server`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },

  setupGhCli: async (attemptId: string): Promise<ExecutionProcess> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/gh-cli-setup`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<ExecutionProcess, GhCliSetupError>(response);
  },

  runSetupScript: async (
    attemptId: string
  ): Promise<Result<ExecutionProcess, RunScriptError>> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/run-setup-script`,
      {
        method: 'POST',
      }
    );
    return handleApiResponseAsResult<ExecutionProcess, RunScriptError>(
      response
    );
  },

  runCleanupScript: async (
    attemptId: string
  ): Promise<Result<ExecutionProcess, RunScriptError>> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/run-cleanup-script`,
      {
        method: 'POST',
      }
    );
    return handleApiResponseAsResult<ExecutionProcess, RunScriptError>(
      response
    );
  },

  getPrComments: async (
    attemptId: string,
    repoId: string
  ): Promise<PrCommentsResponse> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/pr/comments?repo_id=${encodeURIComponent(repoId)}`
    );
    return handleApiResponse<PrCommentsResponse>(response);
  },

  queueMerge: async (
    attemptId: string,
    data: QueueMergeRequest
  ): Promise<Result<MergeQueue, QueueMergeError>> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/queue-merge`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponseAsResult<MergeQueue, QueueMergeError>(response);
  },

  cancelQueuedMerge: async (attemptId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/queue-merge`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  getQueueStatus: async (attemptId: string): Promise<MergeQueue | null> => {
    const response = await makeRequest(
      `/api/task-attempts/${attemptId}/queue-status`
    );
    return handleApiResponse<MergeQueue | null>(response);
  },
};

// Execution Process APIs
export const executionProcessesApi = {
  getDetails: async (processId: string): Promise<ExecutionProcess> => {
    const response = await makeRequest(`/api/execution-processes/${processId}`);
    return handleApiResponse<ExecutionProcess>(response);
  },

  getRepoStates: async (
    processId: string
  ): Promise<ExecutionProcessRepoState[]> => {
    const response = await makeRequest(
      `/api/execution-processes/${processId}/repo-states`
    );
    return handleApiResponse<ExecutionProcessRepoState[]>(response);
  },

  getNormalizedEntries: async (
    processId: string,
    params?: { beforeIndex?: number; limit?: number }
  ): Promise<ExecutionProcessNormalizedEntriesPage> => {
    const query = new URLSearchParams();
    if (params?.beforeIndex !== undefined) {
      query.set('before_index', String(params.beforeIndex));
    }
    if (params?.limit !== undefined) {
      query.set('limit', String(params.limit));
    }
    const suffix = query.toString();
    const response = await makeRequest(
      `/api/execution-processes/${processId}/normalized-entries${
        suffix ? `?${suffix}` : ''
      }`
    );
    return handleApiResponse<ExecutionProcessNormalizedEntriesPage>(response);
  },

  stopExecutionProcess: async (processId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/execution-processes/${processId}/stop`,
      {
        method: 'POST',
      }
    );
    return handleApiResponse<void>(response);
  },
};

// File System APIs
export const fileSystemApi = {
  list: async (path?: string): Promise<DirectoryListResponse> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/filesystem/directory${queryParam}`
    );
    return handleApiResponse<DirectoryListResponse>(response);
  },

  listGitRepos: async (path?: string): Promise<DirectoryEntry[]> => {
    const queryParam = path ? `?path=${encodeURIComponent(path)}` : '';
    const response = await makeRequest(
      `/api/filesystem/git-repos${queryParam}`
    );
    return handleApiResponse<DirectoryEntry[]>(response);
  },
};

// Repo APIs
export const repoApi = {
  register: async (data: {
    path: string;
    display_name?: string;
  }): Promise<Repo> => {
    const response = await makeRequest('/api/repos', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Repo>(response);
  },

  getBranches: async (repoId: string): Promise<GitBranch[]> => {
    const response = await makeRequest(`/api/repos/${repoId}/branches`);
    return handleApiResponse<GitBranch[]>(response);
  },

  createBranch: async (
    repoId: string,
    name: string,
    baseBranch?: string
  ): Promise<GitBranch> => {
    const payload: { name: string; base_branch?: string } = { name };
    if (baseBranch !== undefined) {
      payload.base_branch = baseBranch;
    }
    const response = await makeRequest(`/api/repos/${repoId}/branches`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    return handleApiResponse<GitBranch>(response);
  },

  checkBranchAncestor: async (
    repoId: string,
    branchName: string
  ): Promise<BranchAncestorStatus> => {
    const response = await makeRequest(
      `/api/repos/${repoId}/branches/check-ancestor`,
      {
        method: 'POST',
        body: JSON.stringify({ branch_name: branchName }),
      }
    );
    return handleApiResponse<BranchAncestorStatus>(response);
  },

  init: async (data: {
    parent_path: string;
    folder_name: string;
  }): Promise<Repo> => {
    const response = await makeRequest('/api/repos/init', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Repo>(response);
  },

  clone: async (data: {
    url: string;
    destination?: string;
  }): Promise<Repo> => {
    const response = await makeRequest('/api/repos/clone', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Repo>(response);
  },
};

// Account Info API
export const accountInfoApi = {
  get: async (): Promise<AccountInfo> => {
    const response = await makeRequest('/api/account-info');
    return handleApiResponse<AccountInfo>(response);
  },
};

// Config APIs (backwards compatible)
export const configApi = {
  getConfig: async (): Promise<UserSystemInfo> => {
    const response = await makeRequest('/api/info', { cache: 'no-store' });
    return handleApiResponse<UserSystemInfo>(response);
  },
  saveConfig: async (config: Config): Promise<Config> => {
    const response = await makeRequest('/api/config', {
      method: 'PUT',
      body: JSON.stringify(config),
    });
    return handleApiResponse<Config>(response);
  },
  checkEditorAvailability: async (
    editorType: EditorType
  ): Promise<CheckEditorAvailabilityResponse> => {
    const response = await makeRequest(
      `/api/editors/check-availability?editor_type=${encodeURIComponent(editorType)}`
    );
    return handleApiResponse<CheckEditorAvailabilityResponse>(response);
  },
  checkAgentAvailability: async (
    agent: BaseCodingAgent
  ): Promise<AvailabilityInfo> => {
    const response = await makeRequest(
      `/api/agents/check-availability?executor=${encodeURIComponent(agent)}`
    );
    return handleApiResponse<AvailabilityInfo>(response);
  },
};

// Custom Editors APIs
export const customEditorsApi = {
  list: async (): Promise<ListCustomEditorsResponse> => {
    const response = await makeRequest('/api/config/custom-editors');
    return handleApiResponse<ListCustomEditorsResponse>(response);
  },
  create: async (
    data: CreateCustomEditorRequest
  ): Promise<CustomEditorResponse> => {
    const response = await makeRequest('/api/config/custom-editors', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<CustomEditorResponse>(response);
  },
  update: async (
    editorId: string,
    data: UpdateCustomEditorRequest
  ): Promise<CustomEditorResponse> => {
    const response = await makeRequest(
      `/api/config/custom-editors/${editorId}`,
      {
        method: 'PUT',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<CustomEditorResponse>(response);
  },
  delete: async (editorId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/config/custom-editors/${editorId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },
};

// Task Tags APIs (all tags are global)
export const tagsApi = {
  list: async (params?: TagSearchParams): Promise<Tag[]> => {
    const queryParam = params?.search
      ? `?search=${encodeURIComponent(params.search)}`
      : '';
    const response = await makeRequest(`/api/tags${queryParam}`);
    return handleApiResponse<Tag[]>(response);
  },

  create: async (data: CreateTag): Promise<Tag> => {
    const response = await makeRequest('/api/tags', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Tag>(response);
  },

  update: async (tagId: string, data: UpdateTag): Promise<Tag> => {
    const response = await makeRequest(`/api/tags/${tagId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Tag>(response);
  },

  delete: async (tagId: string): Promise<void> => {
    const response = await makeRequest(`/api/tags/${tagId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },
};

// MCP Servers APIs
export const mcpServersApi = {
  load: async (query: McpServerQuery): Promise<GetMcpServerResponse> => {
    const params = new URLSearchParams(query);
    const response = await makeRequest(`/api/mcp-config?${params.toString()}`);
    return handleApiResponse<GetMcpServerResponse>(response);
  },
  save: async (
    query: McpServerQuery,
    data: UpdateMcpServersBody
  ): Promise<void> => {
    const params = new URLSearchParams(query);
    // params.set('profile', profile);
    const response = await makeRequest(`/api/mcp-config?${params.toString()}`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    if (!response.ok) {
      const errorData = await response.json();
      console.error('[API Error] Failed to save MCP servers', {
        message: errorData.message,
        status: response.status,
        response,
        timestamp: new Date().toISOString(),
      });
      throw new ApiError(
        errorData.message || 'Failed to save MCP servers',
        response.status,
        response
      );
    }
  },
};

// Profiles API
export const profilesApi = {
  load: async (): Promise<{ content: string; path: string }> => {
    const response = await makeRequest('/api/profiles');
    return handleApiResponse<{ content: string; path: string }>(response);
  },
  save: async (content: string): Promise<string> => {
    const response = await makeRequest('/api/profiles', {
      method: 'PUT',
      body: content,
      headers: {
        'Content-Type': 'application/json',
      },
    });
    return handleApiResponse<string>(response);
  },
};

// Images API
export const imagesApi = {
  upload: async (file: File): Promise<ImageResponse> => {
    const baseUrl = await getApiBaseUrl();
    const formData = new FormData();
    formData.append('image', file);

    const response = await fetch(`${baseUrl}/api/images/upload`, {
      method: 'POST',
      body: formData,
      credentials: 'include',
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload image: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<ImageResponse>(response);
  },

  uploadForTask: async (taskId: string, file: File): Promise<ImageResponse> => {
    const baseUrl = await getApiBaseUrl();
    const formData = new FormData();
    formData.append('image', file);

    const response = await fetch(`${baseUrl}/api/images/task/${taskId}/upload`, {
      method: 'POST',
      body: formData,
      credentials: 'include',
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload image: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<ImageResponse>(response);
  },

  /**
   * Upload an image for a task attempt and immediately copy it to the container.
   * Returns the image with a file_path that can be used in markdown.
   */
  uploadForAttempt: async (
    attemptId: string,
    file: File
  ): Promise<ImageResponse> => {
    const baseUrl = await getApiBaseUrl();
    const formData = new FormData();
    formData.append('image', file);

    const response = await fetch(
      `${baseUrl}/api/task-attempts/${attemptId}/images/upload`,
      {
        method: 'POST',
        body: formData,
        credentials: 'include',
      }
    );

    if (!response.ok) {
      const errorText = await response.text();
      throw new ApiError(
        `Failed to upload image: ${errorText}`,
        response.status,
        response
      );
    }

    return handleApiResponse<ImageResponse>(response);
  },

  delete: async (imageId: string): Promise<void> => {
    const response = await makeRequest(`/api/images/${imageId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  getTaskImages: async (taskId: string): Promise<ImageResponse[]> => {
    const response = await makeRequest(`/api/images/task/${taskId}`);
    return handleApiResponse<ImageResponse[]>(response);
  },

  getImageUrl: (imageId: string): string => {
    return `${getApiBaseUrlSync()}/api/images/${imageId}/file`;
  },
};

// Approval API
export const approvalsApi = {
  respond: async (
    approvalId: string,
    payload: ApprovalResponse,
    signal?: AbortSignal
  ): Promise<ApprovalStatus> => {
    const res = await makeRequest(`/api/approvals/${approvalId}/respond`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal,
    });

    return handleApiResponse<ApprovalStatus>(res);
  },
};

// OAuth API
export const oauthApi = {
  handoffInit: async (
    provider: string,
    returnTo: string
  ): Promise<{ handoff_id: string; authorize_url: string }> => {
    const response = await makeRequest('/api/auth/handoff/init', {
      method: 'POST',
      body: JSON.stringify({ provider, return_to: returnTo }),
    });
    return handleApiResponse<{ handoff_id: string; authorize_url: string }>(
      response
    );
  },

  status: async (): Promise<StatusResponse> => {
    const response = await makeRequest('/api/auth/status', {
      cache: 'no-store',
    });
    return handleApiResponse<StatusResponse>(response);
  },

  logout: async (): Promise<void> => {
    const response = await makeRequest('/api/auth/logout', {
      method: 'POST',
    });
    if (!response.ok) {
      throw new ApiError(
        `Logout failed with status ${response.status}`,
        response.status,
        response
      );
    }
  },

  /** Returns the current access token for the remote server (auto-refreshes if needed) */
  getToken: async (): Promise<TokenResponse | null> => {
    const response = await makeRequest('/api/auth/token');
    if (!response.ok) return null;
    return handleApiResponse<TokenResponse>(response);
  },

  /** Returns the user ID of the currently authenticated user */
  getCurrentUser: async (): Promise<CurrentUserResponse> => {
    const response = await makeRequest('/api/auth/user');
    return handleApiResponse<CurrentUserResponse>(response);
  },
};

// Organizations API
export const organizationsApi = {
  getMembers: async (
    orgId: string
  ): Promise<OrganizationMemberWithProfile[]> => {
    const response = await makeRequest(`/api/organizations/${orgId}/members`);
    const result = await handleApiResponse<ListMembersResponse>(response);
    return result.members;
  },

  getUserOrganizations: async (): Promise<ListOrganizationsResponse> => {
    const response = await makeRequest('/api/organizations');
    return handleApiResponse<ListOrganizationsResponse>(response);
  },

  getProjects: async (orgId: string): Promise<RemoteProject[]> => {
    const response = await makeRequest(`/api/organizations/${orgId}/projects`);
    return handleApiResponse<RemoteProject[]>(response);
  },

  createOrganization: async (
    data: CreateOrganizationRequest
  ): Promise<CreateOrganizationResponse> => {
    const response = await makeRequest('/api/organizations', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    return handleApiResponse<CreateOrganizationResponse>(response);
  },

  createInvitation: async (
    orgId: string,
    data: CreateInvitationRequest
  ): Promise<CreateInvitationResponse> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<CreateInvitationResponse>(response);
  },

  removeMember: async (orgId: string, userId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/members/${userId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  updateMemberRole: async (
    orgId: string,
    userId: string,
    data: UpdateMemberRoleRequest
  ): Promise<UpdateMemberRoleResponse> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/members/${userId}/role`,
      {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<UpdateMemberRoleResponse>(response);
  },

  listInvitations: async (orgId: string): Promise<Invitation[]> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations`
    );
    const result = await handleApiResponse<ListInvitationsResponse>(response);
    return result.invitations;
  },

  revokeInvitation: async (
    orgId: string,
    invitationId: string
  ): Promise<void> => {
    const body: RevokeInvitationRequest = { invitation_id: invitationId };
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations/revoke`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }
    );
    return handleApiResponse<void>(response);
  },

  deleteOrganization: async (orgId: string): Promise<void> => {
    const response = await makeRequest(`/api/organizations/${orgId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },
};

// Scratch API
export const scratchApi = {
  create: async (
    scratchType: ScratchType,
    id: string,
    data: CreateScratch
  ): Promise<Scratch> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<Scratch>(response);
  },

  get: async (scratchType: ScratchType, id: string): Promise<Scratch> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`);
    return handleApiResponse<Scratch>(response);
  },

  update: async (
    scratchType: ScratchType,
    id: string,
    data: UpdateScratch
  ): Promise<void> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
    return handleApiResponse<void>(response);
  },

  delete: async (scratchType: ScratchType, id: string): Promise<void> => {
    const response = await makeRequest(`/api/scratch/${scratchType}/${id}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  getStreamUrl: (scratchType: ScratchType, id: string): string => {
    const baseUrl = getApiBaseUrlSync();
    // Convert http(s):// to ws(s):// for WebSocket URLs
    const wsBaseUrl = baseUrl.replace(/^http/, 'ws');
    return `${wsBaseUrl}/api/scratch/${scratchType}/${id}/stream/ws`;
  },
};

// Queue API for session follow-up messages
export const queueApi = {
  /**
   * Queue a follow-up message to be executed when current execution finishes
   */
  queue: async (
    sessionId: string,
    data: { message: string; variant: string | null }
  ): Promise<QueueStatus> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/queue`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return handleApiResponse<QueueStatus>(response);
  },

  /**
   * Cancel a queued follow-up message
   */
  cancel: async (sessionId: string): Promise<QueueStatus> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/queue`, {
      method: 'DELETE',
    });
    return handleApiResponse<QueueStatus>(response);
  },

  /**
   * Get the current queue status for a session
   */
  getStatus: async (sessionId: string): Promise<QueueStatus> => {
    const response = await makeRequest(`/api/sessions/${sessionId}/queue`);
    return handleApiResponse<QueueStatus>(response);
  },
};

// Sounds API for listing available notification sounds
export const soundsApi = {
  list: async (): Promise<AvailableSoundsResponse> => {
    const response = await makeRequest('/api/sounds');
    return handleApiResponse<AvailableSoundsResponse>(response);
  },
};

// Notifications API
export const notificationsApi = {
  list: async (params?: {
    projectId?: string;
    limit?: number;
  }): Promise<Notification[]> => {
    const search = new URLSearchParams();
    if (params?.projectId) {
      search.set('project_id', params.projectId);
    }
    if (params?.limit !== undefined) {
      search.set('limit', params.limit.toString());
    }
    const queryString = search.toString();
    const url = `/api/notifications${queryString ? `?${queryString}` : ''}`;
    const response = await makeRequest(url);
    return handleApiResponse<Notification[]>(response);
  },

  getStats: async (projectId?: string): Promise<NotificationStats> => {
    const search = new URLSearchParams();
    if (projectId) {
      search.set('project_id', projectId);
    }
    const queryString = search.toString();
    const url = `/api/notifications/stats${queryString ? `?${queryString}` : ''}`;
    const response = await makeRequest(url);
    return handleApiResponse<NotificationStats>(response);
  },

  markRead: async (notificationId: string): Promise<Notification> => {
    const update: UpdateNotification = {
      is_read: true,
      title: null,
      message: null,
      metadata: null,
    };
    const response = await makeRequest(`/api/notifications/${notificationId}`, {
      method: 'PATCH',
      body: JSON.stringify(update),
    });
    return handleApiResponse<Notification>(response);
  },

  markAllRead: async (projectId?: string): Promise<number> => {
    const response = await makeRequest('/api/notifications/mark-all-read', {
      method: 'POST',
      body: JSON.stringify({ project_id: projectId ?? null }),
    });
    return handleApiResponse<number>(response);
  },

  delete: async (notificationId: string): Promise<void> => {
    const response = await makeRequest(`/api/notifications/${notificationId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },

  getStreamUrl: (projectId?: string): string => {
    const params = new URLSearchParams();
    if (projectId) {
      params.set('project_id', projectId);
    }
    params.set('include_snapshot', 'true');
    return `/api/notifications/stream/ws?${params.toString()}`;
  },
};

// Conversations API
export interface CreateConversationRequest {
  title: string;
  initial_message: string;
  executor?: string;
}

export interface CreateConversationResponse {
  session: ConversationSession;
  initial_message: ConversationMessage;
}

export interface UpdateConversationRequest {
  title?: string;
  status?: ConversationSessionStatus;
}

export interface SendConversationMessageRequest {
  content: string;
  variant?: string;
}

export const conversationsApi = {
  list: async (projectId: string): Promise<ConversationSession[]> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/conversations?project_id=${encodeURIComponent(projectId)}`
    );
    return handleApiResponse<ConversationSession[]>(response);
  },

  create: async (
    projectId: string,
    data: CreateConversationRequest
  ): Promise<CreateConversationResponse> => {
    const response = await makeRequest(
      `/api/projects/${projectId}/conversations`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<CreateConversationResponse>(response);
  },

  get: async (conversationId: string): Promise<ConversationWithMessages> => {
    const response = await makeRequest(
      `/api/conversations/${conversationId}`
    );
    return handleApiResponse<ConversationWithMessages>(response);
  },

  update: async (
    conversationId: string,
    data: UpdateConversationRequest
  ): Promise<ConversationSession> => {
    const response = await makeRequest(
      `/api/conversations/${conversationId}`,
      {
        method: 'PATCH',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<ConversationSession>(response);
  },

  delete: async (conversationId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/conversations/${conversationId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  getMessages: async (
    conversationId: string,
    params?: { cursor?: string; limit?: number }
  ): Promise<ConversationMessagesPage> => {
    const search = new URLSearchParams();
    if (params?.cursor) {
      search.set('cursor', params.cursor);
    }
    if (params?.limit !== undefined) {
      search.set('limit', params.limit.toString());
    }
    const queryString = search.toString();
    const url = `/api/conversations/${conversationId}/messages${queryString ? `?${queryString}` : ''}`;
    const response = await makeRequest(url);
    return handleApiResponse<ConversationMessagesPage>(response);
  },

  sendMessage: async (
    conversationId: string,
    data: SendConversationMessageRequest
  ): Promise<SendMessageResponse> => {
    const response = await makeRequest(
      `/api/conversations/${conversationId}/messages`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<SendMessageResponse>(response);
  },

  getExecutions: async (
    conversationId: string
  ): Promise<ExecutionProcess[]> => {
    const response = await makeRequest(
      `/api/conversations/${conversationId}/executions`
    );
    return handleApiResponse<ExecutionProcess[]>(response);
  },
};

// GitHub Settings API
export const githubSettingsApi = {
  /** Check if GitHub token is configured */
  getStatus: async (): Promise<GitHubSettingsStatus> => {
    const response = await makeRequest('/api/settings/github');
    return handleApiResponse<GitHubSettingsStatus>(response);
  },

  /** Set GitHub token */
  setToken: async (token: string): Promise<GitHubSettingsStatus> => {
    const response = await makeRequest('/api/settings/github', {
      method: 'PUT',
      body: JSON.stringify({ token }),
    });
    return handleApiResponse<GitHubSettingsStatus>(response);
  },

  /** Delete GitHub token */
  deleteToken: async (): Promise<GitHubSettingsStatus> => {
    const response = await makeRequest('/api/settings/github', {
      method: 'DELETE',
    });
    return handleApiResponse<GitHubSettingsStatus>(response);
  },

  /** Import GitHub token from gh CLI */
  importFromCli: async (): Promise<GitHubImportResponse> => {
    const response = await makeRequest('/api/settings/github/import', {
      method: 'POST',
    });
    return handleApiResponse<GitHubImportResponse>(response);
  },
};

// Feedback API
export const feedbackApi = {
  /** Get all feedback for a task */
  getByTaskId: async (taskId: string): Promise<FeedbackResponse[]> => {
    const response = await makeRequest(`/api/feedback/task/${taskId}`);
    return handleApiResponse<FeedbackResponse[]>(response);
  },

  /** Get all feedback for a workspace (attempt) */
  getByWorkspaceId: async (workspaceId: string): Promise<FeedbackResponse[]> => {
    const response = await makeRequest(`/api/feedback/workspace/${workspaceId}`);
    return handleApiResponse<FeedbackResponse[]>(response);
  },

  /** Get most recent feedback entries */
  getRecent: async (limit?: number): Promise<FeedbackResponse[]> => {
    const params = new URLSearchParams();
    if (limit !== undefined) {
      params.set('limit', limit.toString());
    }
    const queryString = params.toString();
    const url = `/api/feedback/recent${queryString ? `?${queryString}` : ''}`;
    const response = await makeRequest(url);
    return handleApiResponse<FeedbackResponse[]>(response);
  },
};
