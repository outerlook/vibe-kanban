import type { GanttTask, TaskStatus } from '../../../shared/types';

/**
 * Frappe-gantt expected task format
 */
export interface FrappeGanttTask {
  id: string;
  name: string;
  start: string;
  end: string;
  progress: number;
  dependencies: string;
  custom_class?: string;
}

const STATUS_TO_CLASS: Record<TaskStatus, string> = {
  todo: 'gantt-todo',
  inprogress: 'gantt-inprogress',
  inreview: 'gantt-inreview',
  done: 'gantt-done',
  cancelled: 'gantt-cancelled',
};

/**
 * Transform backend GanttTask records to frappe-gantt format
 */
export function transformToFrappeFormat(
  tasks: Record<string, GanttTask>
): FrappeGanttTask[] {
  return Object.values(tasks).map((task) => ({
    id: task.id,
    name: task.name,
    start: task.start,
    end: task.end,
    progress: task.progress,
    dependencies: task.dependencies.join(', '),
    custom_class: STATUS_TO_CLASS[task.task_status],
  }));
}
