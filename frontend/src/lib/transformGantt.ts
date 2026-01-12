import type { GanttTask, TaskStatus } from '../../../shared/types';

/**
 * SVAR Gantt task format
 */
export interface SvarGanttTask {
  id: string;
  text: string;
  start: Date;
  end: Date;
  duration: number;
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

/**
 * Transform backend GanttTask records to SVAR Gantt format
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
    const duration = Math.ceil(
      (end.getTime() - start.getTime()) / (1000 * 60 * 60 * 24)
    );

    svarTasks.push({
      id: task.id,
      text: task.name,
      start,
      end,
      duration,
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
