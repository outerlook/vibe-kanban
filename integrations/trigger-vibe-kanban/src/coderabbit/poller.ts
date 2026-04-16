import type { ScheduledWorkflowDispatchInput } from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';
import type { JsonValue } from '../state/types';
import { VkConfigClient, type SelectedGitHubRepository } from '../vk/config-client';

const DEFAULT_BOOTSTRAP_LOOKBACK_HOURS = 24;
const DEFAULT_MAX_PULL_REQUESTS_PER_REPO = 25;
const DEFAULT_REMEMBERED_COMMENT_IDS = 1000;
const MAX_PULL_REQUESTS_PER_REPO = 100;

const REVIEW_THREADS_QUERY = `query ($owner: String!, $repo: String!, $pullRequestsFirst: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(
      states: OPEN
      first: $pullRequestsFirst
      orderBy: { field: UPDATED_AT, direction: DESC }
    ) {
      nodes {
        number
        title
        url
        updatedAt
        reviewThreads(first: 100) {
          nodes {
            id
            isResolved
            isOutdated
            path
            line
            comments(first: 100) {
              nodes {
                id
                body
                createdAt
                url
                path
                line
                originalLine
                diffHunk
                author {
                  login
                }
              }
            }
          }
        }
      }
    }
  }
}`;

type JsonObject = Record<string, JsonValue>;

export type CodeRabbitPollConfig = {
  workflowId: string;
  reviewerLogins: string[];
  bootstrapLookbackHours: number;
  maxPullRequestsPerRepo: number;
  rememberedCommentIds: number;
  projectIds: string[];
  allowedRepos: string[];
  ignoredRepos: string[];
};

export type CodeRabbitSelectedRepository = {
  repoId: string;
  repoName: string;
  displayName: string;
  path: string;
  githubOwner: string;
  githubRepoName: string;
  githubFullName: string;
  projectIds: string[];
  projectNames: string[];
};

export type CodeRabbitPollCheckpoint = {
  workflowId: string;
  lastPolledAt: string;
  selectedReposByFullName: Record<string, CodeRabbitSelectedRepository>;
  processedThreadCommentIds: string[];
};

type RepoPollRequest = {
  owner: string;
  repo: string;
  pullRequestsFirst: number;
};

type RepoPollResponse = {
  ok: boolean;
  status: number;
  payload: GitHubGraphqlResponse | null;
};

export interface CodeRabbitGitHubClient {
  fetchRepoReviewThreads(request: RepoPollRequest): Promise<RepoPollResponse>;
}

type GitHubGraphqlResponse = {
  data?: {
    repository?: {
      pullRequests?: {
        nodes?: GitHubPullRequest[] | null;
      } | null;
    } | null;
  } | null;
  errors?: Array<{ message?: string | null }> | null;
  error?: { message?: string | null } | string | null;
  message?: string | null;
  statusCode?: number | null;
  httpCode?: number | null;
};

type GitHubPullRequest = {
  number?: number | null;
  title?: string | null;
  url?: string | null;
  updatedAt?: string | null;
  reviewThreads?: {
    nodes?: GitHubReviewThread[] | null;
  } | null;
};

type GitHubReviewThread = {
  id?: string | null;
  isResolved?: boolean | null;
  isOutdated?: boolean | null;
  path?: string | null;
  line?: number | null;
  comments?: {
    nodes?: GitHubReviewComment[] | null;
  } | null;
};

type GitHubReviewComment = {
  id?: string | null;
  body?: string | null;
  createdAt?: string | null;
  url?: string | null;
  path?: string | null;
  line?: number | null;
  originalLine?: number | null;
  diffHunk?: string | null;
  author?: {
    login?: string | null;
  } | null;
};

type InaccessibleRepository = {
  repo: string;
  status: number | null;
  message: string;
};

type PollComment = {
  id: string;
  authorLogin: string;
  body: string;
  createdAt: string;
  url: string;
  path: string;
  line: number | null;
  originalLine: number | null;
  diffHunk: string;
  isNewCoderabbitComment: boolean;
};

type PollThread = {
  threadId: string;
  path: string;
  line: number | null;
  isResolved: boolean;
  isOutdated: boolean;
  matchingCommentCount: number;
  comments: PollComment[];
};

type PollPullRequest = {
  repo: string;
  owner: string;
  repoName: string;
  pullNumber: number | null;
  pullRequestTitle: string;
  pullRequestUrl: string;
  selectedProjects: string[];
  threadCount: number;
  matchingCommentCount: number;
  threads: PollThread[];
};

export type CodeRabbitPollResult = {
  status: 'captured' | 'no_matches';
  workflowId: string;
  pollStartedAt: string;
  previousPollCheckpoint: string | null;
  selectedRepoCount: number;
  selectedRepos: Array<{ repo: string; projects: string[] }>;
  inaccessibleRepos: InaccessibleRepository[];
  inspectedPullRequestCount: number;
  inspectedThreadCount: number;
  pullRequestCount: number;
  matchingCommentCount: number;
  pullRequests: PollPullRequest[];
};

function isJsonObject(value: JsonValue | null | undefined): value is JsonObject {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function parseStringArray(
  value: JsonValue | string | undefined,
  fieldName: string,
): string[] {
  if (value === undefined || value === null) {
    return [];
  }

  if (Array.isArray(value)) {
    return value.map((entry) => String(entry).trim()).filter(Boolean);
  }

  if (typeof value !== 'string') {
    throw new Error(`${fieldName} must be a string or array`);
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return [];
  }

  if (trimmed.startsWith('[')) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(trimmed);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'invalid JSON';
      throw new Error(`Invalid ${fieldName}: ${message}`);
    }

    if (!Array.isArray(parsed)) {
      throw new Error(`${fieldName} must be a JSON array`);
    }

    return parsed.map((entry) => String(entry).trim()).filter(Boolean);
  }

  return trimmed.split(',').map((entry) => entry.trim()).filter(Boolean);
}

function parsePositiveInteger(
  value: JsonValue | string | undefined,
  fieldName: string,
  fallback: number,
): number {
  if (value === undefined || value === null || value === '') {
    return fallback;
  }

  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${fieldName} must be a positive number`);
  }

  return Math.floor(parsed);
}

function normalizeRepository(
  repository: SelectedGitHubRepository,
): CodeRabbitSelectedRepository {
  return {
    repoId: String(repository.repo_id ?? ''),
    repoName: String(repository.repo_name ?? ''),
    displayName: String(repository.display_name ?? ''),
    path: String(repository.path ?? ''),
    githubOwner: String(repository.github_owner ?? ''),
    githubRepoName: String(repository.github_repo_name ?? ''),
    githubFullName: String(repository.github_full_name ?? ''),
    projectIds: repository.projectIds.map((entry) => String(entry)),
    projectNames: repository.projectNames.map((entry) => String(entry)),
  };
}

function readCheckpoint(
  checkpointValue: JsonValue | null,
): CodeRabbitPollCheckpoint | null {
  if (!isJsonObject(checkpointValue)) {
    return null;
  }

  const selectedReposByFullName = isJsonObject(checkpointValue.selectedReposByFullName)
    ? Object.fromEntries(
        Object.entries(checkpointValue.selectedReposByFullName).map(([key, value]) => {
          const repo = isJsonObject(value) ? value : {};
          return [
            key,
            {
              repoId: String(repo.repoId ?? ''),
              repoName: String(repo.repoName ?? ''),
              displayName: String(repo.displayName ?? ''),
              path: String(repo.path ?? ''),
              githubOwner: String(repo.githubOwner ?? ''),
              githubRepoName: String(repo.githubRepoName ?? ''),
              githubFullName: String(repo.githubFullName ?? ''),
              projectIds: Array.isArray(repo.projectIds)
                ? repo.projectIds.map((entry) => String(entry))
                : [],
              projectNames: Array.isArray(repo.projectNames)
                ? repo.projectNames.map((entry) => String(entry))
                : [],
            } satisfies CodeRabbitSelectedRepository,
          ];
        }),
      )
    : {};

  return {
    workflowId: String(checkpointValue.workflowId ?? ''),
    lastPolledAt: String(checkpointValue.lastPolledAt ?? ''),
    selectedReposByFullName,
    processedThreadCommentIds: Array.isArray(checkpointValue.processedThreadCommentIds)
      ? checkpointValue.processedThreadCommentIds.map((entry) => String(entry))
      : [],
  };
}

export function buildCodeRabbitPollMetadataFromEnv(
  env: NodeJS.ProcessEnv = process.env,
): JsonObject {
  const metadata: JsonObject = {};

  const workflowId = env.CODERABBIT_WORKFLOW_ID?.trim();
  if (workflowId) {
    metadata.workflowId = workflowId;
  }

  const reviewerLogins = parseStringArray(
    env.CODERABBIT_REVIEWER_LOGINS,
    'CODERABBIT_REVIEWER_LOGINS',
  );
  if (reviewerLogins.length > 0) {
    metadata.reviewerLogins = reviewerLogins;
  }

  const projectIds = parseStringArray(env.CODERABBIT_PROJECT_IDS, 'CODERABBIT_PROJECT_IDS');
  if (projectIds.length > 0) {
    metadata.projectIds = projectIds;
  }

  const allowedRepos = parseStringArray(
    env.CODERABBIT_ALLOWED_REPOS,
    'CODERABBIT_ALLOWED_REPOS',
  );
  if (allowedRepos.length > 0) {
    metadata.allowedRepos = allowedRepos;
  }

  const ignoredRepos = parseStringArray(
    env.CODERABBIT_IGNORED_REPOS,
    'CODERABBIT_IGNORED_REPOS',
  );
  if (ignoredRepos.length > 0) {
    metadata.ignoredRepos = ignoredRepos;
  }

  if (env.CODERABBIT_BOOTSTRAP_LOOKBACK_HOURS?.trim()) {
    metadata.bootstrapLookbackHours = parsePositiveInteger(
      env.CODERABBIT_BOOTSTRAP_LOOKBACK_HOURS,
      'CODERABBIT_BOOTSTRAP_LOOKBACK_HOURS',
      DEFAULT_BOOTSTRAP_LOOKBACK_HOURS,
    );
  }

  if (env.CODERABBIT_MAX_PULL_REQUESTS_PER_REPO?.trim()) {
    metadata.maxPullRequestsPerRepo = Math.min(
      parsePositiveInteger(
        env.CODERABBIT_MAX_PULL_REQUESTS_PER_REPO,
        'CODERABBIT_MAX_PULL_REQUESTS_PER_REPO',
        DEFAULT_MAX_PULL_REQUESTS_PER_REPO,
      ),
      MAX_PULL_REQUESTS_PER_REPO,
    );
  }

  if (env.CODERABBIT_REMEMBERED_COMMENT_IDS?.trim()) {
    metadata.rememberedCommentIds = parsePositiveInteger(
      env.CODERABBIT_REMEMBERED_COMMENT_IDS,
      'CODERABBIT_REMEMBERED_COMMENT_IDS',
      DEFAULT_REMEMBERED_COMMENT_IDS,
    );
  }

  return metadata;
}

export function resolveCodeRabbitPollConfig(
  metadata: JsonValue | null,
  env: NodeJS.ProcessEnv = process.env,
): CodeRabbitPollConfig {
  const values = isJsonObject(metadata) ? metadata : {};
  const workflowId = String(values.workflowId ?? env.CODERABBIT_WORKFLOW_ID ?? '').trim();
  if (!workflowId) {
    throw new Error('CodeRabbit polling requires workflowId');
  }

  const reviewerLogins = Array.from(
    new Set(
      parseStringArray(
        values.reviewerLogins ?? env.CODERABBIT_REVIEWER_LOGINS,
        'reviewerLogins',
      ).map((entry) => entry.toLowerCase()),
    ),
  );
  if (reviewerLogins.length === 0) {
    throw new Error('CodeRabbit polling requires at least one reviewer login');
  }

  return {
    workflowId,
    reviewerLogins,
    bootstrapLookbackHours: parsePositiveInteger(
      values.bootstrapLookbackHours ?? env.CODERABBIT_BOOTSTRAP_LOOKBACK_HOURS,
      'bootstrapLookbackHours',
      DEFAULT_BOOTSTRAP_LOOKBACK_HOURS,
    ),
    maxPullRequestsPerRepo: Math.min(
      parsePositiveInteger(
        values.maxPullRequestsPerRepo ?? env.CODERABBIT_MAX_PULL_REQUESTS_PER_REPO,
        'maxPullRequestsPerRepo',
        DEFAULT_MAX_PULL_REQUESTS_PER_REPO,
      ),
      MAX_PULL_REQUESTS_PER_REPO,
    ),
    rememberedCommentIds: parsePositiveInteger(
      values.rememberedCommentIds ?? env.CODERABBIT_REMEMBERED_COMMENT_IDS,
      'rememberedCommentIds',
      DEFAULT_REMEMBERED_COMMENT_IDS,
    ),
    projectIds: parseStringArray(
      values.projectIds ?? env.CODERABBIT_PROJECT_IDS,
      'projectIds',
    ),
    allowedRepos: parseStringArray(
      values.allowedRepos ?? env.CODERABBIT_ALLOWED_REPOS,
      'allowedRepos',
    ).map((entry) => entry.toLowerCase()),
    ignoredRepos: parseStringArray(
      values.ignoredRepos ?? env.CODERABBIT_IGNORED_REPOS,
      'ignoredRepos',
    ).map((entry) => entry.toLowerCase()),
  };
}

export class GitHubGraphqlCodeRabbitClient implements CodeRabbitGitHubClient {
  private readonly endpoint: string;
  private readonly token: string;

  constructor(env: NodeJS.ProcessEnv = process.env) {
    const token = env.GITHUB_TOKEN?.trim();
    if (!token) {
      throw new Error('CodeRabbit polling requires GITHUB_TOKEN for GitHub GraphQL access');
    }

    this.endpoint = env.GITHUB_GRAPHQL_URL?.trim() || 'https://api.github.com/graphql';
    this.token = token;
  }

  async fetchRepoReviewThreads(request: RepoPollRequest): Promise<RepoPollResponse> {
    const response = await fetch(this.endpoint, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${this.token}`,
        'user-agent': 'vibe-kanban-trigger-coderabbit',
      },
      body: JSON.stringify({
        query: REVIEW_THREADS_QUERY,
        variables: {
          owner: request.owner,
          repo: request.repo,
          pullRequestsFirst: Math.min(request.pullRequestsFirst, MAX_PULL_REQUESTS_PER_REPO),
        },
      }),
    });

    let payload: GitHubGraphqlResponse | null = null;
    try {
      payload = (await response.json()) as GitHubGraphqlResponse;
    } catch {
      payload = null;
    }

    return {
      ok: response.ok,
      status: response.status,
      payload,
    };
  }
}

function recordInaccessibleRepository(
  inaccessibleRepos: InaccessibleRepository[],
  repository: CodeRabbitSelectedRepository,
  status: number | null,
  message: string,
): void {
  inaccessibleRepos.push({
    repo: repository.githubFullName,
    status,
    message: message || 'unknown GitHub error',
  });
}

function readMessage(value: GitHubGraphqlResponse | null): string {
  if (!value) {
    return 'GitHub request failed';
  }

  if (typeof value.message === 'string' && value.message.trim()) {
    return value.message;
  }

  if (typeof value.error === 'string' && value.error.trim()) {
    return value.error;
  }

  if (value.error && typeof value.error === 'object' && typeof value.error.message === 'string') {
    return value.error.message;
  }

  return 'GitHub request failed';
}

export async function runCodeRabbitPollWorkflow(
  input: ScheduledWorkflowDispatchInput,
  dependencies: RuntimeDependencies,
  options?: {
    config?: CodeRabbitPollConfig;
    configClient?: Pick<VkConfigClient, 'listGitHubRepositories'>;
    githubClient?: CodeRabbitGitHubClient;
  },
): Promise<{ output: CodeRabbitPollResult; nextCheckpoint: CodeRabbitPollCheckpoint }> {
  const config = options?.config ?? resolveCodeRabbitPollConfig(input.claim.metadata);
  const configClient =
    options?.configClient ?? new VkConfigClient(dependencies.environment.vkApi);
  const githubClient =
    options?.githubClient ?? new GitHubGraphqlCodeRabbitClient();

  const checkpoint = readCheckpoint(input.checkpoint?.checkpoint ?? null);
  const pollStartedAt = input.scheduledAt;
  const bootstrapAnchor = new Date(
    Date.parse(pollStartedAt) - config.bootstrapLookbackHours * 60 * 60 * 1000,
  ).toISOString();
  const pollAfter = checkpoint?.lastPolledAt || bootstrapAnchor;
  const pollAfterMs = Date.parse(pollAfter);
  const knownCommentIds = new Set(
    checkpoint?.processedThreadCommentIds.map((entry) => String(entry)) ?? [],
  );
  const newCommentIds = [] as string[];
  const inaccessibleRepos = [] as InaccessibleRepository[];
  const pullRequests = [] as PollPullRequest[];
  let inspectedPullRequestCount = 0;
  let inspectedThreadCount = 0;

  const selectedRepositories = (
    await configClient.listGitHubRepositories({
      workflowId: config.workflowId,
      projectIds: config.projectIds,
      allowedRepos: config.allowedRepos,
      ignoredRepos: config.ignoredRepos,
    })
  ).map(normalizeRepository);

  for (const repository of selectedRepositories) {
    let repoResponse: RepoPollResponse;
    try {
      repoResponse = await githubClient.fetchRepoReviewThreads({
        owner: repository.githubOwner,
        repo: repository.githubRepoName,
        pullRequestsFirst: config.maxPullRequestsPerRepo,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'GitHub request failed';
      recordInaccessibleRepository(inaccessibleRepos, repository, null, message);
      continue;
    }

    if (Array.isArray(repoResponse.payload?.errors) && repoResponse.payload.errors.length > 0) {
      recordInaccessibleRepository(
        inaccessibleRepos,
        repository,
        null,
        repoResponse.payload.errors
          .map((entry) => entry?.message || 'unknown GraphQL error')
          .join(', '),
      );
      continue;
    }

    if (!repoResponse.ok || repoResponse.payload?.message === 'Not Found') {
      recordInaccessibleRepository(
        inaccessibleRepos,
        repository,
        repoResponse.payload?.statusCode ?? repoResponse.payload?.httpCode ?? repoResponse.status,
        readMessage(repoResponse.payload),
      );
      continue;
    }

    const repoPullRequests = repoResponse.payload?.data?.repository?.pullRequests?.nodes ?? [];
    inspectedPullRequestCount += repoPullRequests.length;

    for (const pullRequest of repoPullRequests) {
      const updatedAtMs = Date.parse(String(pullRequest?.updatedAt ?? ''));
      if (
        Number.isFinite(updatedAtMs) &&
        Number.isFinite(pollAfterMs) &&
        updatedAtMs < pollAfterMs
      ) {
        continue;
      }

      const unresolvedThreads = (pullRequest?.reviewThreads?.nodes ?? []).filter(
        (thread): thread is GitHubReviewThread => Boolean(thread) && !thread.isResolved,
      );
      inspectedThreadCount += unresolvedThreads.length;

      const matchingThreads = [] as PollThread[];
      for (const thread of unresolvedThreads) {
        const matchingCommentIds = [] as string[];
        const comments = (thread.comments?.nodes ?? []).map((comment) => {
          const createdAtMs = Date.parse(String(comment?.createdAt ?? ''));
          const authorLogin = String(comment?.author?.login ?? '');
          const id = String(comment?.id ?? '');
          const isNewCoderabbitComment =
            config.reviewerLogins.includes(authorLogin.toLowerCase()) &&
            Number.isFinite(createdAtMs) &&
            Number.isFinite(pollAfterMs) &&
            createdAtMs >= pollAfterMs &&
            !knownCommentIds.has(id);

          if (isNewCoderabbitComment) {
            matchingCommentIds.push(id);
            knownCommentIds.add(id);
            newCommentIds.push(id);
          }

          return {
            id,
            authorLogin,
            body: String(comment?.body ?? ''),
            createdAt: String(comment?.createdAt ?? ''),
            url: String(comment?.url ?? ''),
            path: String(comment?.path ?? thread.path ?? ''),
            line:
              typeof comment?.line === 'number'
                ? comment.line
                : typeof comment?.originalLine === 'number'
                  ? comment.originalLine
                  : typeof thread.line === 'number'
                    ? thread.line
                    : null,
            originalLine:
              typeof comment?.originalLine === 'number' ? comment.originalLine : null,
            diffHunk: String(comment?.diffHunk ?? ''),
            isNewCoderabbitComment,
          } satisfies PollComment;
        });

        if (matchingCommentIds.length === 0) {
          continue;
        }

        matchingThreads.push({
          threadId: String(thread.id ?? ''),
          path: String(thread.path ?? ''),
          line: typeof thread.line === 'number' ? thread.line : null,
          isResolved: Boolean(thread.isResolved),
          isOutdated: Boolean(thread.isOutdated),
          matchingCommentCount: matchingCommentIds.length,
          comments,
        });
      }

      if (matchingThreads.length === 0) {
        continue;
      }

      pullRequests.push({
        repo: repository.githubFullName,
        owner: repository.githubOwner,
        repoName: repository.githubRepoName,
        pullNumber: typeof pullRequest?.number === 'number' ? pullRequest.number : null,
        pullRequestTitle: String(pullRequest?.title ?? ''),
        pullRequestUrl: String(pullRequest?.url ?? ''),
        selectedProjects: repository.projectNames,
        threadCount: matchingThreads.length,
        matchingCommentCount: matchingThreads.reduce(
          (total, thread) => total + thread.matchingCommentCount,
          0,
        ),
        threads: matchingThreads,
      });
    }
  }

  // Keep the freshest matches first so the remembered dedup window does not evict
  // the just-processed comment ids when older checkpoints are large.
  const nextProcessedThreadCommentIds = Array.from(
    new Set([...newCommentIds, ...(checkpoint?.processedThreadCommentIds ?? [])]),
  ).slice(0, config.rememberedCommentIds);

  const selectedReposByFullName = Object.fromEntries(
    selectedRepositories.map((repository) => [
      repository.githubFullName.toLowerCase(),
      repository,
    ]),
  );

  const output: CodeRabbitPollResult = {
    status: pullRequests.length > 0 ? 'captured' : 'no_matches',
    workflowId: config.workflowId,
    pollStartedAt,
    previousPollCheckpoint: pollAfter,
    selectedRepoCount: selectedRepositories.length,
    selectedRepos: selectedRepositories.map((repository) => ({
      repo: repository.githubFullName,
      projects: repository.projectNames,
    })),
    inaccessibleRepos,
    inspectedPullRequestCount,
    inspectedThreadCount,
    pullRequestCount: pullRequests.length,
    matchingCommentCount: pullRequests.reduce(
      (total, pullRequest) => total + pullRequest.matchingCommentCount,
      0,
    ),
    pullRequests,
  };

  return {
    output,
    nextCheckpoint: {
      workflowId: config.workflowId,
      lastPolledAt: pollStartedAt,
      selectedReposByFullName,
      processedThreadCommentIds: nextProcessedThreadCommentIds,
    },
  };
}
