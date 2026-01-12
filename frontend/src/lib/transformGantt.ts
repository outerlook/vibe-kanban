import type { GanttTask, TaskStatus } from '../../../shared/types';
import { getTaskGroupColorClass } from './ganttColors';

/**
 * Options for transforming Gantt tasks
 */
export interface TransformOptions {
  colorMode?: 'status' | 'group';
  defaultDuration?: number; // minutes, default 30
}

/**
 * SVAR Gantt task format
 * Note: SVAR expects either end OR duration, not both.
 * We use start+end since that's what the backend provides.
 * The `type` field controls bar color via taskTypes config.
 * When colorMode is 'group', type will be a group color class (e.g., 'group-0', 'ungrouped').
 */
export interface SvarGanttTask {
  id: string;
  text: string;
  start: Date;
  end: Date;
  progress: number;
  type: TaskStatus | string;
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

const ONE_MINUTE_MS = 60 * 1000;
const DEFAULT_DURATION_MINUTES = 30;

/**
 * Detect if a task is "unstarted" - has no meaningful execution history.
 * This is indicated by start === end or duration < 1 minute.
 */
function isUnstartedTask(task: GanttTask): boolean {
  const start = new Date(task.start).getTime();
  const end = new Date(task.end).getTime();
  return end - start < ONE_MINUTE_MS;
}

/**
 * Topological sort with cycle detection.
 * Returns task IDs in order such that dependencies come before dependents.
 */
function topologicalSort(
  tasks: Record<string, GanttTask>
): string[] {
  const visited = new Set<string>();
  const inStack = new Set<string>();
  const result: string[] = [];

  function visit(taskId: string, path: string[]): void {
    if (inStack.has(taskId)) {
      const cycleStart = path.indexOf(taskId);
      const cycle = [...path.slice(cycleStart), taskId];
      throw new Error(`Circular dependency detected: ${cycle.join(' → ')}`);
    }
    if (visited.has(taskId)) return;

    const task = tasks[taskId];
    if (!task) return;

    inStack.add(taskId);
    for (const depId of task.dependencies) {
      visit(depId, [...path, taskId]);
    }
    inStack.delete(taskId);
    visited.add(taskId);
    result.push(taskId);
  }

  for (const taskId of Object.keys(tasks)) {
    visit(taskId, []);
  }

  return result;
}

/**
 * Calculate positions for unstarted tasks.
 * - Tasks with no dependencies: start at current time
 * - Tasks with dependencies: start after max(dependency.end) + 1 minute buffer
 *
 * @returns Map of task ID to computed { start, end } dates
 */
function calculateUnstartedTaskPositions(
  tasks: Record<string, GanttTask>,
  defaultDurationMinutes: number
): Map<string, { start: Date; end: Date }> {
  const positions = new Map<string, { start: Date; end: Date }>();
  const now = Date.now();
  const defaultDurationMs = defaultDurationMinutes * ONE_MINUTE_MS;
  const bufferMs = ONE_MINUTE_MS;

  // Process in topological order so dependencies are calculated first
  const sortedIds = topologicalSort(tasks);

  for (const taskId of sortedIds) {
    const task = tasks[taskId];
    if (!task) continue;

    // If task already has execution history, use its original times
    if (!isUnstartedTask(task)) {
      positions.set(taskId, {
        start: new Date(task.start),
        end: new Date(task.end),
      });
      continue;
    }

    // For unstarted tasks, calculate position based on dependencies
    let startTime = now;

    for (const depId of task.dependencies) {
      const depPosition = positions.get(depId);
      if (depPosition) {
        const depEndWithBuffer = depPosition.end.getTime() + bufferMs;
        startTime = Math.max(startTime, depEndWithBuffer);
      }
    }

    positions.set(taskId, {
      start: new Date(startTime),
      end: new Date(startTime + defaultDurationMs),
    });
  }

  return positions;
}

/**
 * Transform backend GanttTask records to SVAR Gantt format.
 */
export function transformToSvarFormat(
  tasks: Record<string, GanttTask>,
  options: TransformOptions = {}
): {
  tasks: SvarGanttTask[];
  links: SvarGanttLink[];
} {
  const { colorMode = 'status', defaultDuration = DEFAULT_DURATION_MINUTES } = options;
  const svarTasks: SvarGanttTask[] = [];
  const links: SvarGanttLink[] = [];

  // Calculate positions for all tasks (handles unstarted task positioning)
  const positions = calculateUnstartedTaskPositions(tasks, defaultDuration);

  for (const task of Object.values(tasks)) {
    const position = positions.get(task.id);
    const start = position?.start ?? new Date(task.start);
    const end = position?.end ?? new Date(task.end);

    const type = colorMode === 'group'
      ? getTaskGroupColorClass(task.task_group_id)
      : task.task_status;

    svarTasks.push({
      id: task.id,
      text: task.name,
      start,
      end,
      progress: task.progress,
      type,
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
