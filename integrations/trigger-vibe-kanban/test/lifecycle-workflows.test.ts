import { afterEach, describe, expect, it } from 'bun:test';
import { connect } from 'mqtt';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { join } from 'node:path';

import { ORCHESTRATION_SCHEMA_VERSION } from '../../../shared/orchestration-events';

import { createDirectDispatcher } from '../src/trigger/direct-dispatcher';
import { createConsoleLogger } from '../src/runtime/dependencies';
import { TRIGGER_EXECUTOR_OPERATION_KEYS } from '../src/runtime/executor-mapping';
import { startMqttBridgeRuntime } from '../src/runtime/mqtt-bridge';
import { createSqliteStateStore } from '../src/state/sqlite-state-store';
import { VkRuntimeClient } from '../src/vk/runtime-client';
import {
  createBroker,
  createJsonServer,
  createTempDir,
  createTestOpenClawSessionConfigResolver,
  createTestTriggerExecutorMapping,
  removeTempDir,
} from './helpers';

const tempDirs: string[] = [];

afterEach(() => {
  while (tempDirs.length > 0) {
    const next = tempDirs.pop();
    if (next) {
      removeTempDir(next);
    }
  }
});

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];

  for await (const chunk of request) {
    chunks.push(typeof chunk === 'string' ? Buffer.from(chunk) : chunk);
  }

  return JSON.parse(Buffer.concat(chunks).toString());
}

function respondWithEnvelope(response: ServerResponse, data: unknown): void {
  response.setHeader('content-type', 'application/json');
  response.end(
    JSON.stringify({
      success: true,
      data,
    }),
  );
}

describe('lifecycle workflows', () => {
  it('continues autopilot dependents for done tasks and ignores replayed MQTT delivery', async () => {
    const broker = await createBroker();
    const startExecutionBodies: unknown[] = [];
    let taskContextReads = 0;

    const httpServer = await createJsonServer(async (request, response) => {
      if (
        request.method === 'GET' &&
        request.url === '/api/tasks/task-done/orchestration-context'
      ) {
        taskContextReads += 1;
        respondWithEnvelope(response, {
          task: {
            id: 'task-done',
          },
          dependency_context: {
            blocked_by: [],
            dependents: [
              {
                id: 'dep-runnable-1',
                title: 'Runnable 1',
                status: 'todo',
                task_group_id: 'tg-1',
                is_blocked: false,
                has_in_progress_attempt: false,
                is_queued: false,
                needs_attention: null,
              },
              {
                id: 'dep-blocked',
                title: 'Blocked',
                status: 'todo',
                task_group_id: 'tg-1',
                is_blocked: true,
                has_in_progress_attempt: false,
                is_queued: false,
                needs_attention: null,
              },
              {
                id: 'dep-running',
                title: 'Running',
                status: 'todo',
                task_group_id: 'tg-1',
                is_blocked: false,
                has_in_progress_attempt: true,
                is_queued: false,
                needs_attention: null,
              },
              {
                id: 'dep-queued',
                title: 'Queued',
                status: 'todo',
                task_group_id: 'tg-1',
                is_blocked: false,
                has_in_progress_attempt: false,
                is_queued: true,
                needs_attention: null,
              },
              {
                id: 'dep-done',
                title: 'Done',
                status: 'done',
                task_group_id: 'tg-1',
                is_blocked: false,
                has_in_progress_attempt: false,
                is_queued: false,
                needs_attention: null,
              },
              {
                id: 'dep-runnable-2',
                title: 'Runnable 2',
                status: 'todo',
                task_group_id: 'tg-1',
                is_blocked: false,
                has_in_progress_attempt: false,
                is_queued: false,
                needs_attention: null,
              },
            ],
          },
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/task-groups/tg-1/orchestration-context'
      ) {
        respondWithEnvelope(response, {
          task_group: {
            id: 'tg-1',
          },
          stats: {
            todo: 0,
            in_progress: 0,
            in_review: 0,
            done: 1,
            cancelled: 0,
          },
          tasks: [],
          dependency_context: {
            blocked_tasks: [],
          },
          queue_state: {
            queued_tasks: [],
            merge_queue_entries: [],
          },
        });
        return;
      }

      if (
        request.method === 'POST' &&
        request.url === '/api/task-attempts/orchestration/task-executions'
      ) {
        startExecutionBodies.push(await readJsonBody(request));
        respondWithEnvelope(response, {
          status: 'started',
        });
        return;
      }

      response.statusCode = 404;
      response.end('not found');
    });

    const tempDir = createTempDir('trigger-lifecycle-');
    tempDirs.push(tempDir);

    const dependencies = {
      environment: {
        vkApi: {
          baseUrl: `http://127.0.0.1:${httpServer.port}`,
          authMode: 'none' as const,
        },
        vkMqtt: {
          brokerUrl: `mqtt://127.0.0.1:${broker.port}`,
          topicNamespace: 'vk/orchestration',
        },
        stateDatabasePath: join(tempDir, 'state.sqlite'),
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        codeRabbitPollScopeKey: 'global',
        mqttRouterMode: 'direct' as const,
        triggerExecutorMapping: createTestTriggerExecutorMapping(),
        openClawSessionConfigResolver: createTestOpenClawSessionConfigResolver(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: `http://127.0.0.1:${httpServer.port}`,
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
    };

    const runtime = await startMqttBridgeRuntime({
      dependencies,
      eventTypes: ['task_status_changed'],
      dispatcher: createDirectDispatcher(dependencies),
    });

    const publisher = connect(dependencies.environment.vkMqtt.brokerUrl);
    await new Promise<void>((resolve, reject) => {
      publisher.once('connect', () => resolve());
      publisher.once('error', (error) => reject(error));
    });

    const payload = JSON.stringify({
      event_id: 'evt-task-done-1',
      schema_version: ORCHESTRATION_SCHEMA_VERSION,
      occurred_at: '2026-04-16T10:00:00Z',
      event_type: 'task_status_changed',
      task_id: 'task-done',
      workspace_id: null,
      session_id: null,
      execution_process_id: null,
      task_group_id: 'tg-1',
      payload: {
        project_id: 'project-1',
        previous_status: 'in_progress',
        status: 'done',
      },
    });

    publisher.publish('vk/orchestration/task_status_changed', payload);
    await Bun.sleep(150);
    publisher.publish('vk/orchestration/task_status_changed', payload);
    await Bun.sleep(150);

    const explicitAutopilotExecutor =
      dependencies.environment.triggerExecutorMapping.resolve(
        TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution,
      );

    expect(taskContextReads).toBe(1);
    expect(startExecutionBodies).toEqual([
      {
        task_id: 'dep-runnable-1',
        workspace_strategy: 'latest_or_create',
        executor_strategy: {
          executor_selection: 'explicit',
          executor_profile_id: explicitAutopilotExecutor,
        },
        repo_selection: {
          repo_selection: 'task_group_default',
        },
      },
      {
        task_id: 'dep-runnable-2',
        workspace_strategy: 'latest_or_create',
        executor_strategy: {
          executor_selection: 'explicit',
          executor_profile_id: explicitAutopilotExecutor,
        },
        repo_selection: {
          repo_selection: 'task_group_default',
        },
      },
    ]);
    expect(dependencies.stateStore.getClaim('evt-task-done-1')?.status).toBe(
      'completed',
    );

    await runtime.close();
    dependencies.stateStore.close();
    publisher.end(true);
    await broker.close();
    await httpServer.close();
  });

  it('collects feedback once for task-scoped execution completion despite replayed delivery', async () => {
    const broker = await createBroker();
    const feedbackBodies: unknown[] = [];
    let executionContextReads = 0;

    const httpServer = await createJsonServer(async (request, response) => {
      if (
        request.method === 'GET' &&
        request.url === '/api/execution-processes/exec-1/orchestration-context'
      ) {
        executionContextReads += 1;
        respondWithEnvelope(response, {
          execution: {
            id: 'exec-1',
            status: 'completed',
          },
          scope: {
            task: {
              id: 'task-1',
            },
            workspace: {
              id: 'workspace-1',
            },
            session: null,
            conversation: null,
          },
          coding_agent_turn: null,
          repo_states: [],
          current_execution_visibility: null,
          pending_tool_approvals: [],
          pending_questions: [],
          review_attention: {
            id: 'attention-1',
            needs_attention: true,
          },
          feedback: {
            id: 'feedback-existing-1',
            feedback: {
              score: 'good',
            },
          },
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/tasks/task-1/orchestration-context'
      ) {
        respondWithEnvelope(response, {
          task: {
            id: 'task-1',
          },
          images: [],
          latest_workspace: null,
          latest_session: null,
          latest_coding_execution: null,
          current_execution_visibility: null,
          pending_tool_approvals: [],
          pending_questions: [],
          dependency_context: {
            blocked_by: [],
            dependents: [],
          },
          latest_review_attention: null,
          latest_feedback: null,
          queue_state: {
            execution_queue: null,
            merge_queue: null,
          },
        });
        return;
      }

      if (request.method === 'POST' && request.url === '/api/feedback') {
        feedbackBodies.push(await readJsonBody(request));
        respondWithEnvelope(response, {
          id: 'feedback-1',
        });
        return;
      }

      response.statusCode = 404;
      response.end('not found');
    });

    const tempDir = createTempDir('trigger-lifecycle-');
    tempDirs.push(tempDir);

    const dependencies = {
      environment: {
        vkApi: {
          baseUrl: `http://127.0.0.1:${httpServer.port}`,
          authMode: 'none' as const,
        },
        vkMqtt: {
          brokerUrl: `mqtt://127.0.0.1:${broker.port}`,
          topicNamespace: 'vk/orchestration',
        },
        stateDatabasePath: join(tempDir, 'state.sqlite'),
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        codeRabbitPollScopeKey: 'global',
        mqttRouterMode: 'direct' as const,
        triggerExecutorMapping: createTestTriggerExecutorMapping(),
        openClawSessionConfigResolver: createTestOpenClawSessionConfigResolver(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: `http://127.0.0.1:${httpServer.port}`,
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
    };

    const runtime = await startMqttBridgeRuntime({
      dependencies,
      eventTypes: ['execution_completed'],
      dispatcher: createDirectDispatcher(dependencies),
    });

    const publisher = connect(dependencies.environment.vkMqtt.brokerUrl);
    await new Promise<void>((resolve, reject) => {
      publisher.once('connect', () => resolve());
      publisher.once('error', (error) => reject(error));
    });

    const payload = JSON.stringify({
      event_id: 'evt-exec-complete-1',
      schema_version: ORCHESTRATION_SCHEMA_VERSION,
      occurred_at: '2026-04-16T11:00:00Z',
      event_type: 'execution_completed',
      task_id: 'task-1',
      workspace_id: 'workspace-1',
      session_id: 'session-1',
      execution_process_id: 'exec-1',
      task_group_id: null,
      payload: {
        status: 'completed',
        run_reason: 'task_execution',
        exit_code: 0,
        conversation_session_id: null,
      },
    });

    publisher.publish('vk/orchestration/execution_completed', payload);
    await Bun.sleep(150);
    publisher.publish('vk/orchestration/execution_completed', payload);
    await Bun.sleep(150);

    expect(executionContextReads).toBe(1);
    expect(feedbackBodies).toEqual([
      {
        task_id: 'task-1',
        workspace_id: 'workspace-1',
        execution_process_id: 'exec-1',
        feedback_json: JSON.stringify({
          summary: 'completed',
          review_attention: {
            id: 'attention-1',
            needs_attention: true,
          },
          feedback: {
            id: 'feedback-existing-1',
            feedback: {
              score: 'good',
            },
          },
        }),
      },
    ]);
    expect(dependencies.stateStore.getClaim('evt-exec-complete-1')?.status).toBe(
      'completed',
    );

    await runtime.close();
    dependencies.stateStore.close();
    publisher.end(true);
    await broker.close();
    await httpServer.close();
  });

  it('skips feedback collection for conversation-only execution scope', async () => {
    const broker = await createBroker();
    const feedbackBodies: unknown[] = [];

    const httpServer = await createJsonServer(async (request, response) => {
      if (
        request.method === 'GET' &&
        request.url === '/api/execution-processes/exec-conversation/orchestration-context'
      ) {
        respondWithEnvelope(response, {
          execution: {
            id: 'exec-conversation',
            status: 'completed',
          },
          scope: {
            task: null,
            workspace: null,
            session: null,
            conversation: {
              id: 'conversation-1',
            },
          },
          coding_agent_turn: null,
          repo_states: [],
          current_execution_visibility: null,
          pending_tool_approvals: [],
          pending_questions: [],
          review_attention: null,
          feedback: null,
        });
        return;
      }

      if (
        request.method === 'GET' &&
        request.url === '/api/conversations/conversation-1/orchestration-context'
      ) {
        respondWithEnvelope(response, {
          conversation: {
            id: 'conversation-1',
          },
          transcript: {
            messages: [],
            images: [],
          },
          executions: [],
          current_execution_visibility: null,
          latest_agent_session_id: null,
        });
        return;
      }

      if (request.method === 'POST' && request.url === '/api/feedback') {
        feedbackBodies.push(await readJsonBody(request));
        response.statusCode = 500;
        response.end('unexpected create feedback call');
        return;
      }

      response.statusCode = 404;
      response.end('not found');
    });

    const tempDir = createTempDir('trigger-lifecycle-');
    tempDirs.push(tempDir);

    const dependencies = {
      environment: {
        vkApi: {
          baseUrl: `http://127.0.0.1:${httpServer.port}`,
          authMode: 'none' as const,
        },
        vkMqtt: {
          brokerUrl: `mqtt://127.0.0.1:${broker.port}`,
          topicNamespace: 'vk/orchestration',
        },
        stateDatabasePath: join(tempDir, 'state.sqlite'),
        schemaVersion: ORCHESTRATION_SCHEMA_VERSION,
        codeRabbitPollScopeKey: 'global',
        mqttRouterMode: 'direct' as const,
        triggerExecutorMapping: createTestTriggerExecutorMapping(),
        openClawSessionConfigResolver: createTestOpenClawSessionConfigResolver(),
      },
      stateStore: createSqliteStateStore(join(tempDir, 'state.sqlite')),
      vkClient: new VkRuntimeClient({
        baseUrl: `http://127.0.0.1:${httpServer.port}`,
        authMode: 'none',
      }),
      logger: createConsoleLogger(),
    };

    const runtime = await startMqttBridgeRuntime({
      dependencies,
      eventTypes: ['execution_completed'],
      dispatcher: createDirectDispatcher(dependencies),
    });

    const publisher = connect(dependencies.environment.vkMqtt.brokerUrl);
    await new Promise<void>((resolve, reject) => {
      publisher.once('connect', () => resolve());
      publisher.once('error', (error) => reject(error));
    });

    publisher.publish(
      'vk/orchestration/execution_completed',
      JSON.stringify({
        event_id: 'evt-exec-conversation-1',
        schema_version: ORCHESTRATION_SCHEMA_VERSION,
        occurred_at: '2026-04-16T12:00:00Z',
        event_type: 'execution_completed',
        task_id: null,
        workspace_id: null,
        session_id: null,
        execution_process_id: 'exec-conversation',
        task_group_id: null,
        payload: {
          status: 'completed',
          run_reason: 'conversation',
          exit_code: 0,
          conversation_session_id: 'conversation-1',
        },
      }),
    );

    await Bun.sleep(150);

    expect(feedbackBodies).toEqual([]);
    expect(
      dependencies.stateStore.getClaim('evt-exec-conversation-1')?.status,
    ).toBe('completed');

    await runtime.close();
    dependencies.stateStore.close();
    publisher.end(true);
    await broker.close();
    await httpServer.close();
  });
});
