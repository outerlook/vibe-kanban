import type { OrchestrationWorkflowHandler } from '../types';
import type { TaskListItemDto } from '../../vk/types';
import { TRIGGER_EXECUTOR_OPERATION_KEYS } from '../../runtime/executor-mapping';

const WORKFLOW_KEY = 'lifecycle/autopilot-continuation';

function isRunnableDependent(task: TaskListItemDto): boolean {
  return (
    task.status === 'todo' &&
    !task.is_blocked &&
    !task.has_in_progress_attempt &&
    !task.is_queued
  );
}

export const autopilotContinuationHandler: OrchestrationWorkflowHandler = {
  workflowKey: WORKFLOW_KEY,
  matches(input) {
    return input.event.eventType === 'task_status_changed';
  },
  async run(input, deps) {
    const payload = input.event.payload as { status?: string };
    if (payload.status !== 'done') {
      return {
        status: 'skipped',
        reason: 'task_not_done',
        taskId: input.event.entityIds.taskId,
        nextStatus: payload.status,
      };
    }

    const taskContext = input.contexts.task;
    if (!taskContext) {
      return {
        status: 'skipped',
        reason: 'missing_task_context',
        taskId: input.event.entityIds.taskId,
      };
    }

    const runnableDependents = taskContext.dependency_context.dependents.filter(
      isRunnableDependent,
    );
    const executorProfileId = deps.environment.triggerExecutorMapping.resolve(
      TRIGGER_EXECUTOR_OPERATION_KEYS.lifecycleAutopilotStartTaskExecution,
    );

    for (const dependentTask of runnableDependents) {
      await deps.vkClient.startTaskExecution({
        task_id: dependentTask.id,
        workspace_strategy: 'latest_or_create',
        executor_strategy: {
          executor_selection: 'explicit',
          executor_profile_id: executorProfileId,
        },
        repo_selection: {
          repo_selection: 'task_group_default',
        },
      });
    }

    return {
      status: 'handled',
      completedTaskId: taskContext.task.id,
      startedTaskIds: runnableDependents.map((task) => task.id),
      skippedDependentCount:
        taskContext.dependency_context.dependents.length - runnableDependents.length,
    };
  },
};
