import type { GanttTask, TaskStatus } from '../../../shared/types';

/**
 * SVAR Gantt task format
 * Note: SVAR expects either end OR duration, not both.
 * We use start+end since that's what the backend provides.
 */
export interface SvarGanttTask {
  id: string;
  text: string;
  start: Date;
  end: Date;
  progress: number;
  type: 'task';
  taskStatus: TaskStatus;
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

const ONE_HOUR_MS = 60 * 60 * 1000;

/**
 * Transform backend GanttTask records to SVAR Gantt format.
 * Ensures minimum 1-hour duration for visibility (tasks with start == end are invisible).
 */
export function transformToSvarFormat(tasks: Record<string, GanttTask>): {
  tasks: SvarGanttTask[];
  links: SvarGanttLink[];
} {
  const svarTasks: SvarGanttTask[] = [];
  const links: SvarGanttLink[] = [];

  for (const task of Object.values(tasks)) {
    const start = new Date(task.start);
    let end = new Date(task.end);

    // Ensure minimum 1-hour duration for visibility (zero-duration tasks are invisible)
    if (end.getTime() - start.getTime() < ONE_HOUR_MS) {
      end = new Date(start.getTime() + ONE_HOUR_MS);
    }

    svarTasks.push({
      id: task.id,
      text: task.name,
      start,
      end,
      progress: task.progress,
      type: 'task',
      taskStatus: task.task_status,
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
