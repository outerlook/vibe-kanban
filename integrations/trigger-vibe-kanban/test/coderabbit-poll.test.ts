import { afterEach, describe, expect, it } from 'bun:test';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { join } from 'node:path';

import { ORCHESTRATION_SCHEMA_VERSION } from '../../../shared/orchestration-events';

import {
  createDirectDispatcher,
  createSqliteStateStore,
  runScheduledWorkflowOnce,
  VkRuntimeClient,
} from '../src';
import { createConsoleLogger } from '../src/runtime/dependencies';
import {
  createJsonServer,
  createTempDir,
  createTestTriggerExecutorMapping,
  removeTempDir,
} from './helpers';

const tempDirs = [] as string[];

const previousGitHubToken = process.env.GITHUB_TOKEN;
const previousGitHubGraphqlUrl = process.env.GITHUB_GRAPHQL_URL;

afterEach(() => {
  while (tempDirs.length > 0) {
    const next = tempDirs.pop();
    if (next) {
      removeTempDir(next);
    }
  }

  if (previousGitHubToken === undefined) {
    delete process.env.GITHUB_TOKEN;
  } else {
    process.env.GITHUB_TOKEN = previousGitHubToken;
  }

  if (previousGitHubGraphqlUrl === undefined) {
    delete process.env.GITHUB_GRAPHQL_URL;
  } else {
    process.env.GITHUB_GRAPHQL_URL = previousGitHubGraphqlUrl;
  }
});

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks = [] as Uint8Array[];
  for await (const chunk of request) {
    chunks.push(typeof chunk === 'string' ? Buffer.from(chunk) : chunk);
  }

  const body = Buffer.concat(chunks).toString('utf8').trim();
  return body ? JSON.parse(body) : null;
}

function writeJson(response: ServerResponse, data: unknown, statusCode = 200): void {
  response.statusCode = statusCode;
  response.setHeader('content-type', 'application/json');
  response.end(JSON.stringify(data));
}

function createDependencies(tempDir: string, vkBaseUrl: string) {
  return {
    environment: {
      vkApi: {
        baseUrl: vkBaseUrl,
        authMode: 'none' as const,
      },
      vkMqtt: {
        brokerUrl: 'mqtt://127.0.0.1:1883',
        topicNamespace: 'vk/orchestration',
      },
      stateDatabasePath: join(tempDir, 'state.sqlite'),
      schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
      codeRabbitPollScopeKey: 'global',
      mqttRouterMode: 'direct' as const,
      triggerExecutorMapping: createTestTriggerExecutorMapping(),
    },
    stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
    vkClient: new VkRuntimeClient({
      baseUrl: vkBaseUrl,
      authMode: 'none',
    }),
    logger: createConsoleLogger(),
  };
}

describe('coderabbit scheduled poller', () => {
  it('uses VK workflow selection and captures only new unresolved CodeRabbit comments', async () => {
    const githubRequests = [] as string[];

    const vkServer = await createJsonServer((request, response) => {
      if (request.method === 'GET' && request.url === '/api/projects') {
        writeJson(response, {
          success: true,
          data: [
            { id: 'project-1', name: 'Alpha' },
            { id: 'project-2', name: 'Beta' },
            { id: 'project-3', name: 'Gamma' },
          ],
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-1/workflow-association'
      ) {
        writeJson(response, {
          success: true,
          data: { workflow_id: 'wf-coderabbit-review-extraction' },
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-2/workflow-association'
      ) {
        writeJson(response, { success: true, data: { workflow_id: 'wf-other' } });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-3/workflow-association'
      ) {
        writeJson(response, {
          success: true,
          data: { workflow_id: 'wf-coderabbit-review-extraction' },
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-1/github-repositories'
      ) {
        writeJson(response, {
          success: true,
          data: [
            {
              repo_id: 'repo-1',
              repo_name: 'Repo One',
              display_name: 'Repo One',
              path: 'repos/repo-one',
              github_owner: 'acme',
              github_repo_name: 'repo-one',
              github_full_name: 'acme/repo-one',
            },
          ],
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-2/github-repositories'
      ) {
        writeJson(response, {
          success: true,
          data: [
            {
              repo_id: 'repo-2',
              repo_name: 'Repo Two',
              display_name: 'Repo Two',
              path: 'repos/repo-two',
              github_owner: 'acme',
              github_repo_name: 'repo-two',
              github_full_name: 'acme/repo-two',
            },
          ],
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-3/github-repositories'
      ) {
        writeJson(response, {
          success: true,
          data: [
            {
              repo_id: 'repo-3',
              repo_name: 'Repo Three',
              display_name: 'Repo Three',
              path: 'repos/repo-three',
              github_owner: 'acme',
              github_repo_name: 'repo-three',
              github_full_name: 'acme/repo-three',
            },
          ],
        });
        return;
      }

      response.statusCode = 404;
      response.end('not found');
    });

    const githubServer = await createJsonServer(async (request, response) => {
      if (request.method !== 'POST' || request.url !== '/graphql') {
        response.statusCode = 404;
        response.end('not found');
        return;
      }

      const body = (await readJsonBody(request)) as {
        variables?: { owner?: string; repo?: string };
      };
      const owner = body.variables?.owner ?? '';
      const repo = body.variables?.repo ?? '';
      githubRequests.push(`${owner}/${repo}`);

      writeJson(response, {
        data: {
          repository: {
            pullRequests: {
              nodes: [
                {
                  number: 101,
                  title: 'Improve polling integration',
                  url: 'https://github.com/acme/repo-one/pull/101',
                  updatedAt: '2026-04-16T11:00:00Z',
                  reviewThreads: {
                    nodes: [
                      {
                        id: 'thread-1',
                        isResolved: false,
                        isOutdated: false,
                        path: 'src/main.ts',
                        line: 17,
                        comments: {
                          nodes: [
                            {
                              id: 'comment-old',
                              body: 'Old bot comment',
                              createdAt: '2026-04-15T11:00:00Z',
                              url: 'https://github.com/acme/repo-one/pull/101#discussion_r1',
                              path: 'src/main.ts',
                              line: 17,
                              originalLine: 17,
                              diffHunk: '@@ -1,1 +1,1 @@',
                              author: { login: 'coderabbitai[bot]' },
                            },
                            {
                              id: 'comment-new',
                              body: 'Please tighten this branch condition.',
                              createdAt: '2026-04-16T10:30:00Z',
                              url: 'https://github.com/acme/repo-one/pull/101#discussion_r2',
                              path: 'src/main.ts',
                              line: 18,
                              originalLine: 18,
                              diffHunk: '@@ -5,1 +5,1 @@',
                              author: { login: 'coderabbitai[bot]' },
                            },
                            {
                              id: 'comment-human',
                              body: 'Human reviewer note',
                              createdAt: '2026-04-16T10:45:00Z',
                              url: 'https://github.com/acme/repo-one/pull/101#discussion_r3',
                              path: 'src/main.ts',
                              line: 19,
                              originalLine: 19,
                              diffHunk: '@@ -7,1 +7,1 @@',
                              author: { login: 'octocat' },
                            },
                          ],
                        },
                      },
                      {
                        id: 'thread-resolved',
                        isResolved: true,
                        isOutdated: false,
                        path: 'src/skip.ts',
                        line: 2,
                        comments: {
                          nodes: [
                            {
                              id: 'comment-resolved',
                              body: 'Resolved thread comment',
                              createdAt: '2026-04-16T10:35:00Z',
                              url: 'https://github.com/acme/repo-one/pull/101#discussion_r4',
                              path: 'src/skip.ts',
                              line: 2,
                              originalLine: 2,
                              diffHunk: '@@ -1,1 +1,1 @@',
                              author: { login: 'coderabbitai[bot]' },
                            },
                          ],
                        },
                      },
                    ],
                  },
                },
                {
                  number: 88,
                  title: 'Older pull request',
                  url: 'https://github.com/acme/repo-one/pull/88',
                  updatedAt: '2026-04-15T10:00:00Z',
                  reviewThreads: {
                    nodes: [
                      {
                        id: 'thread-old-pr',
                        isResolved: false,
                        isOutdated: false,
                        path: 'src/legacy.ts',
                        line: 4,
                        comments: { nodes: [] },
                      },
                    ],
                  },
                },
              ],
            },
          },
        },
      });
    });

    process.env.GITHUB_TOKEN = 'test-token';
    process.env.GITHUB_GRAPHQL_URL = `http://127.0.0.1:${githubServer.port}/graphql`;

    const tempDir = createTempDir('trigger-coderabbit-');
    tempDirs.push(tempDir);
    const dependencies = createDependencies(tempDir, `http://127.0.0.1:${vkServer.port}`);

    try {
      const result = await runScheduledWorkflowOnce(
        {
          workflowKey: 'coderabbit/poll',
          scopeKey: 'global',
          claimKey: 'coderabbit:poll-1',
          scheduledAt: '2026-04-16T12:00:00Z',
          metadata: {
            workflowId: 'wf-coderabbit-review-extraction',
            reviewerLogins: ['coderabbitai[bot]'],
            ignoredRepos: ['acme/repo-three'],
          },
        },
        dependencies,
        createDirectDispatcher(dependencies),
      );

      expect(githubRequests).toEqual(['acme/repo-one']);
      expect(result.disposition).toBe('handled');
      expect(result.handlerKeys).toEqual(['coderabbit/poll']);
      expect(result.output).toEqual({
        status: 'captured',
        workflowId: 'wf-coderabbit-review-extraction',
        pollStartedAt: '2026-04-16T12:00:00Z',
        previousPollCheckpoint: '2026-04-15T12:00:00.000Z',
        selectedRepoCount: 1,
        selectedRepos: [{ repo: 'acme/repo-one', projects: ['Alpha'] }],
        inaccessibleRepos: [],
        inspectedPullRequestCount: 2,
        inspectedThreadCount: 1,
        pullRequestCount: 1,
        matchingCommentCount: 1,
        pullRequests: [
          {
            repo: 'acme/repo-one',
            owner: 'acme',
            repoName: 'repo-one',
            pullNumber: 101,
            pullRequestTitle: 'Improve polling integration',
            pullRequestUrl: 'https://github.com/acme/repo-one/pull/101',
            selectedProjects: ['Alpha'],
            threadCount: 1,
            matchingCommentCount: 1,
            threads: [
              {
                threadId: 'thread-1',
                path: 'src/main.ts',
                line: 17,
                isResolved: false,
                isOutdated: false,
                matchingCommentCount: 1,
                comments: [
                  {
                    id: 'comment-old',
                    authorLogin: 'coderabbitai[bot]',
                    body: 'Old bot comment',
                    createdAt: '2026-04-15T11:00:00Z',
                    url: 'https://github.com/acme/repo-one/pull/101#discussion_r1',
                    path: 'src/main.ts',
                    line: 17,
                    originalLine: 17,
                    diffHunk: '@@ -1,1 +1,1 @@',
                    isNewCoderabbitComment: false,
                  },
                  {
                    id: 'comment-new',
                    authorLogin: 'coderabbitai[bot]',
                    body: 'Please tighten this branch condition.',
                    createdAt: '2026-04-16T10:30:00Z',
                    url: 'https://github.com/acme/repo-one/pull/101#discussion_r2',
                    path: 'src/main.ts',
                    line: 18,
                    originalLine: 18,
                    diffHunk: '@@ -5,1 +5,1 @@',
                    isNewCoderabbitComment: true,
                  },
                  {
                    id: 'comment-human',
                    authorLogin: 'octocat',
                    body: 'Human reviewer note',
                    createdAt: '2026-04-16T10:45:00Z',
                    url: 'https://github.com/acme/repo-one/pull/101#discussion_r3',
                    path: 'src/main.ts',
                    line: 19,
                    originalLine: 19,
                    diffHunk: '@@ -7,1 +7,1 @@',
                    isNewCoderabbitComment: false,
                  },
                ],
              },
            ],
          },
        ],
      });
      expect(dependencies.stateStore.getCheckpoint('coderabbit/poll', 'global')?.checkpoint).toEqual({
        workflowId: 'wf-coderabbit-review-extraction',
        lastPolledAt: '2026-04-16T12:00:00Z',
        selectedReposByFullName: {
          'acme/repo-one': {
            repoId: 'repo-1',
            repoName: 'Repo One',
            displayName: 'Repo One',
            path: 'repos/repo-one',
            githubOwner: 'acme',
            githubRepoName: 'repo-one',
            githubFullName: 'acme/repo-one',
            projectIds: ['project-1'],
            projectNames: ['Alpha'],
          },
        },
        processedThreadCommentIds: ['comment-new'],
      });
    } finally {
      dependencies.stateStore.close();
      await vkServer.close();
      await githubServer.close();
    }
  });

  it('keeps repeated scheduler runs replay-safe by checkpointing handled comment ids', async () => {
    const vkServer = await createJsonServer((request, response) => {
      if (request.method === 'GET' && request.url === '/api/projects') {
        writeJson(response, {
          success: true,
          data: [{ id: 'project-1', name: 'Alpha' }],
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-1/workflow-association'
      ) {
        writeJson(response, {
          success: true,
          data: { workflow_id: 'wf-coderabbit-review-extraction' },
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/projects/project-1/github-repositories'
      ) {
        writeJson(response, {
          success: true,
          data: [
            {
              repo_id: 'repo-1',
              repo_name: 'Repo One',
              display_name: 'Repo One',
              path: 'repos/repo-one',
              github_owner: 'acme',
              github_repo_name: 'repo-one',
              github_full_name: 'acme/repo-one',
            },
          ],
        });
        return;
      }

      response.statusCode = 404;
      response.end('not found');
    });

    const githubServer = await createJsonServer((_request, response) => {
      writeJson(response, {
        data: {
          repository: {
            pullRequests: {
              nodes: [
                {
                  number: 101,
                  title: 'Improve polling integration',
                  url: 'https://github.com/acme/repo-one/pull/101',
                  updatedAt: '2026-04-16T12:30:00Z',
                  reviewThreads: {
                    nodes: [
                      {
                        id: 'thread-1',
                        isResolved: false,
                        isOutdated: false,
                        path: 'src/main.ts',
                        line: 17,
                        comments: {
                          nodes: [
                            {
                              id: 'comment-new',
                              body: 'Please tighten this branch condition.',
                              createdAt: '2026-04-16T12:15:00Z',
                              url: 'https://github.com/acme/repo-one/pull/101#discussion_r2',
                              path: 'src/main.ts',
                              line: 18,
                              originalLine: 18,
                              diffHunk: '@@ -5,1 +5,1 @@',
                              author: { login: 'coderabbitai[bot]' },
                            },
                          ],
                        },
                      },
                    ],
                  },
                },
              ],
            },
          },
        },
      });
    });

    process.env.GITHUB_TOKEN = 'test-token';
    process.env.GITHUB_GRAPHQL_URL = `http://127.0.0.1:${githubServer.port}/graphql`;

    const tempDir = createTempDir('trigger-coderabbit-');
    tempDirs.push(tempDir);
    const dependencies = createDependencies(tempDir, `http://127.0.0.1:${vkServer.port}`);

    try {
      const firstRun = await runScheduledWorkflowOnce(
        {
          workflowKey: 'coderabbit/poll',
          scopeKey: 'global',
          claimKey: 'coderabbit:poll-1',
          scheduledAt: '2026-04-16T12:20:00Z',
          metadata: {
            workflowId: 'wf-coderabbit-review-extraction',
            reviewerLogins: ['coderabbitai[bot]'],
            rememberedCommentIds: 1,
          },
        },
        dependencies,
        createDirectDispatcher(dependencies),
      );
      const secondRun = await runScheduledWorkflowOnce(
        {
          workflowKey: 'coderabbit/poll',
          scopeKey: 'global',
          claimKey: 'coderabbit:poll-2',
          scheduledAt: '2026-04-16T13:20:00Z',
          metadata: {
            workflowId: 'wf-coderabbit-review-extraction',
            reviewerLogins: ['coderabbitai[bot]'],
            rememberedCommentIds: 1,
          },
        },
        dependencies,
        createDirectDispatcher(dependencies),
      );

      expect((firstRun.output as { matchingCommentCount: number }).matchingCommentCount).toBe(1);
      expect(secondRun.output).toEqual({
        status: 'no_matches',
        workflowId: 'wf-coderabbit-review-extraction',
        pollStartedAt: '2026-04-16T13:20:00Z',
        previousPollCheckpoint: '2026-04-16T12:20:00Z',
        selectedRepoCount: 1,
        selectedRepos: [{ repo: 'acme/repo-one', projects: ['Alpha'] }],
        inaccessibleRepos: [],
        inspectedPullRequestCount: 1,
        inspectedThreadCount: 1,
        pullRequestCount: 0,
        matchingCommentCount: 0,
        pullRequests: [],
      });
      expect(dependencies.stateStore.getCheckpoint('coderabbit/poll', 'global')?.checkpoint).toEqual({
        workflowId: 'wf-coderabbit-review-extraction',
        lastPolledAt: '2026-04-16T13:20:00Z',
        selectedReposByFullName: {
          'acme/repo-one': {
            repoId: 'repo-1',
            repoName: 'Repo One',
            displayName: 'Repo One',
            path: 'repos/repo-one',
            githubOwner: 'acme',
            githubRepoName: 'repo-one',
            githubFullName: 'acme/repo-one',
            projectIds: ['project-1'],
            projectNames: ['Alpha'],
          },
        },
        processedThreadCommentIds: ['comment-new'],
      });
    } finally {
      dependencies.stateStore.close();
      await vkServer.close();
      await githubServer.close();
    }
  });
});
