import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import net from 'node:net';
import { once } from 'node:events';

import aedes from 'aedes';
import { connect } from 'mqtt';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import {
  answerApproval,
  createConversation,
  getExecutorProfiles,
  getApprovalContext,
} from '../nodes/VibeKanban/shared/api';
import {
  VK_ORCHESTRATION_SCHEMA_VERSION,
  type VkEventType,
} from '../nodes/VibeKanban/shared/constants';
import {
  closeVkMqttClient,
  createVkMqttClient,
  parseVkOrchestrationEvent,
  topicForEvent,
} from '../nodes/VibeKanban/shared/events';
import type {
  VkApiCredentialValue,
  VkMqttCredentialValue,
} from '../nodes/VibeKanban/shared/vk-contracts';

function listen(server: net.Server | ReturnType<typeof createServer>) {
  return new Promise<number>((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (!address || typeof address === 'string') {
        throw new Error('Failed to read server address');
      }
      resolve(address.port);
    });
  });
}

describe('VK n8n smoke harness', () => {
  const broker = aedes();
  const mqttServer = net.createServer(broker.handle);
  let httpServer: ReturnType<typeof createServer>;
  let mqttPort = 0;
  let httpPort = 0;
  let receivedApprovalBody: unknown = null;
  let receivedCreateConversationBody: unknown = null;

  const approvalContext = {
    approval: {
      id: 'approval-1',
      kind: 'tool_approval',
      status: 'pending',
      execution_process_id: 'exec-1',
      tool_call_id: 'tool-call-1',
      tool_name: 'agent_browser',
      tool_input: { url: 'https://example.com' },
      questions: [],
      answers: [],
      created_at: '2026-04-14T10:00:00Z',
      timeout_at: null,
      answered_at: null,
    },
    execution: null,
    task: null,
    workspace: null,
    session: null,
    current_execution_visibility: null,
  };

  let apiCredentials: VkApiCredentialValue;
  let mqttCredentials: VkMqttCredentialValue;

  beforeAll(async () => {
    mqttPort = await listen(mqttServer);

    httpServer = createServer(
      async (request: IncomingMessage, response: ServerResponse) => {
        if (
          request.method === 'GET' &&
          request.url === '/api/profiles'
        ) {
          response.setHeader('content-type', 'application/json');
          response.end(
            JSON.stringify({
              success: true,
              data: {
                content: JSON.stringify({
                  executors: {
                    CLAUDE_CODE: {
                      DEFAULT: {
                        CLAUDE_CODE: {
                          dangerously_skip_permissions: true,
                        },
                      },
                      PLAN: {
                        CLAUDE_CODE: {
                          plan: true,
                        },
                      },
                    },
                    CODEX: {
                      DEFAULT: {
                        CODEX: {
                          model: 'gpt-5.2-codex',
                        },
                      },
                      HIGH: {
                        CODEX: {
                          model: 'gpt-5.2-codex',
                          model_reasoning_effort: 'high',
                        },
                      },
                    },
                  },
                }),
                path: '/tmp/profiles.json',
              },
            }),
          );
          return;
        }

        if (
          request.method === 'POST' &&
          request.url === '/api/projects/project-1/conversations'
        ) {
          const chunks: Buffer[] = [];
          for await (const chunk of request) {
            chunks.push(Buffer.from(chunk));
          }
          receivedCreateConversationBody = JSON.parse(Buffer.concat(chunks).toString());
          response.setHeader('content-type', 'application/json');
          response.end(
            JSON.stringify({
              success: true,
              data: {
                session: {
                  id: 'conversation-1',
                  project_id: 'project-1',
                  title: 'Investigate webhook failure',
                },
                initial_message: {
                  id: 'message-1',
                  role: 'user',
                  content: 'Investigate the webhook failure.',
                },
                execution_process_id: 'exec-2',
              },
            }),
          );
          return;
        }

        if (
          request.method === 'GET' &&
          request.url === '/api/approvals/approval-1/orchestration-context'
        ) {
          response.setHeader('content-type', 'application/json');
          response.end(
            JSON.stringify({
              success: true,
              data: approvalContext,
            }),
          );
          return;
        }

        if (
          request.method === 'POST' &&
          request.url === '/api/approvals/approval-1/respond'
        ) {
          const chunks: Buffer[] = [];
          for await (const chunk of request) {
            chunks.push(Buffer.from(chunk));
          }
          receivedApprovalBody = JSON.parse(Buffer.concat(chunks).toString());
          response.setHeader('content-type', 'application/json');
          response.end(JSON.stringify({ status: 'approved' }));
          return;
        }

        response.statusCode = 404;
        response.end('not found');
      },
    );

    httpPort = await listen(httpServer);

    apiCredentials = {
      baseUrl: `http://127.0.0.1:${httpPort}`,
      authMode: 'none',
    };
    mqttCredentials = {
      brokerUrl: `mqtt://127.0.0.1:${mqttPort}`,
      topicNamespace: 'vk/orchestration',
    };
  });

  afterAll(async () => {
    broker.close();
    mqttServer.close();
    httpServer.close();
  });

  it('receives an MQTT event, hydrates approval context, and answers it via VK surfaces', async () => {
    const subscriber = await createVkMqttClient(mqttCredentials);
    const topic = topicForEvent(
      mqttCredentials.topicNamespace,
      'approval_requested' satisfies VkEventType,
    );

    await new Promise<void>((resolve, reject) => {
      subscriber.subscribe(topic, (error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve();
      });
    });

    const messagePromise = once(subscriber, 'message').then(([receivedTopic, payload]) =>
      parseVkOrchestrationEvent(
        payload as Buffer,
        receivedTopic as string,
        VK_ORCHESTRATION_SCHEMA_VERSION,
      ),
    );

    const publisher = connect(mqttCredentials.brokerUrl);
    await once(publisher, 'connect');
    publisher.publish(
      topic,
      JSON.stringify({
        event_id: 'evt-1',
        schema_version: VK_ORCHESTRATION_SCHEMA_VERSION,
        occurred_at: '2026-04-14T10:00:00Z',
        event_type: 'approval_requested',
        task_id: null,
        workspace_id: null,
        session_id: null,
        execution_process_id: 'exec-1',
        task_group_id: null,
        payload: {
          approval_id: 'approval-1',
          kind: 'tool_approval',
          tool_call_id: 'tool-call-1',
          tool_name: 'agent_browser',
          question_count: 0,
        },
      }),
    );

    const event = await messagePromise;
    const hydrated = await getApprovalContext(apiCredentials, event.refs.approvalContext!.approvalId);
    const answer = await answerApproval(apiCredentials, hydrated.approval.id, {
      execution_process_id: hydrated.approval.execution_process_id,
      status: { status: 'approved' },
    });

    expect(event.eventType).toBe('approval_requested');
    expect(hydrated.approval.id).toBe('approval-1');
    expect(answer).toEqual({ status: 'approved' });
    expect(receivedApprovalBody).toEqual({
      execution_process_id: 'exec-1',
      status: { status: 'approved' },
    });

    publisher.end(true);
    await closeVkMqttClient(subscriber);
  });

  it('creates conversations through the VK control plane', async () => {
    const response = await createConversation(apiCredentials, 'project-1', {
      title: 'Investigate webhook failure',
      initial_message: 'Investigate the webhook failure.',
      executor_profile_id: {
        executor: 'CODEX',
        variant: 'HIGH',
      },
      worktree_path: 'services/webhooks',
      worktree_branch: 'feature/webhook-debug',
    });

    expect(response.execution_process_id).toBe('exec-2');
    expect(response.session.id).toBe('conversation-1');
    expect(receivedCreateConversationBody).toEqual({
      title: 'Investigate webhook failure',
      initial_message: 'Investigate the webhook failure.',
      executor_profile_id: {
        executor: 'CODEX',
        variant: 'HIGH',
      },
      worktree_path: 'services/webhooks',
      worktree_branch: 'feature/webhook-debug',
    });
  });

  it('loads executor profiles through the VK control plane', async () => {
    const profiles = await getExecutorProfiles(apiCredentials);

    expect(Object.keys(profiles.executors)).toEqual(['CLAUDE_CODE', 'CODEX']);
    expect(Object.keys(profiles.executors.CODEX ?? {})).toEqual([
      'DEFAULT',
      'HIGH',
    ]);
  });
});
