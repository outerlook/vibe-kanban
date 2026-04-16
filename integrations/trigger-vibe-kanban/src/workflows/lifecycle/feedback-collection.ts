import type { OrchestrationWorkflowHandler } from '../types';

const WORKFLOW_KEY = 'lifecycle/feedback-collection';

export const feedbackCollectionHandler: OrchestrationWorkflowHandler = {
  workflowKey: WORKFLOW_KEY,
  matches(input) {
    return input.event.eventType === 'execution_completed';
  },
  async run(input, deps) {
    const executionContext = input.contexts.execution;
    if (!executionContext) {
      return {
        status: 'skipped',
        reason: 'missing_execution_context',
        executionProcessId: input.event.entityIds.executionProcessId,
      };
    }

    const taskId = executionContext.scope.task?.id;
    const workspaceId = executionContext.scope.workspace?.id;
    if (!taskId || !workspaceId) {
      return {
        status: 'skipped',
        reason: 'execution_scope_not_task_workspace',
        executionProcessId: executionContext.execution.id,
      };
    }

    await deps.vkClient.createFeedback({
      task_id: taskId,
      workspace_id: workspaceId,
      execution_process_id: executionContext.execution.id,
      feedback_json: JSON.stringify({
        summary: executionContext.execution.status,
        review_attention: executionContext.review_attention,
        feedback: executionContext.feedback,
      }),
    });

    return {
      status: 'handled',
      taskId,
      workspaceId,
      executionProcessId: executionContext.execution.id,
    };
  },
};
