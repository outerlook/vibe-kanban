import type { TaskStatus } from 'shared/types';

/**
 * All task statuses in display order.
 * This is the canonical source for status ordering across the app.
 */
export const TASK_STATUSES: readonly TaskStatus[] = [
  'todo',
  'inprogress',
  'inreview',
  'done',
  'cancelled',
] as const;

/**
 * Normalize a status string to a valid TaskStatus.
 * Lowercases and casts to TaskStatus.
 */
export const normalizeStatus = (status: string): TaskStatus =>
  status.toLowerCase() as TaskStatus;
