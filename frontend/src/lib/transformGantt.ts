import type { GanttTask, TaskStatus } from '../../../shared/types';

/**
 * SVAR Gantt task format
 * Note: SVAR expects either end OR duration, not both.
 * We use start+end since that's what the backend provides.
 * The `type` field controls bar color via taskTypes config.
 */
export interface SvarGanttTask {
  id: string;
  text: string;
  start: Date;
  end: Date;
  progress: number;
  type: TaskStatus;
  totalInputTokens: number | null;
  totalOutputTokens: number | null;
}

/**
 * SVAR Gantt link format for dependencies (end-to-start)
 */
export interface SvarGanttLink {
  id: string;
  source: string;
  target: string;
  type: 'e2s';
}

/**
 * Transform backend GanttTask records to SVAR Gantt format.
 */
export function transformToSvarFormat(tasks: Record<string, GanttTask>): {
  tasks: SvarGanttTask[];
  links: SvarGanttLink[];
} {
  const svarTasks: SvarGanttTask[] = [];
  const links: SvarGanttLink[] = [];

  for (const task of Object.values(tasks)) {
    const start = new Date(task.start);
    const end = new Date(task.end);

    svarTasks.push({
      id: task.id,
      text: task.name,
      start,
      end,
      progress: task.progress,
      type: task.task_status,
      totalInputTokens:
        task.total_input_tokens != null
          ? Number(task.total_input_tokens)
          : null,
      totalOutputTokens:
        task.total_output_tokens != null
          ? Number(task.total_output_tokens)
          : null,
    });

    task.dependencies.forEach((depId, index) => {
      links.push({
        id: `${task.id}-${depId}-${index}`,
        source: depId,
        target: task.id,
        type: 'e2s',
      });
    });
  }

  return { tasks: svarTasks, links };
}
