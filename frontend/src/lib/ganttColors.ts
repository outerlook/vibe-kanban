/**
 * Color palette for task groups in Gantt charts.
 * Each group gets a consistent color based on its ID hash.
 */

export interface TaskGroupColor {
  color: string;
  fill: string;
  border: string;
}

export const TASK_GROUP_PALETTE: TaskGroupColor[] = [
  { color: '#6366f1', fill: '#4f46e5', border: '#4338ca' }, // Indigo
  { color: '#8b5cf6', fill: '#7c3aed', border: '#6d28d9' }, // Violet
  { color: '#ec4899', fill: '#db2777', border: '#be185d' }, // Pink
  { color: '#f97316', fill: '#ea580c', border: '#c2410c' }, // Orange
  { color: '#14b8a6', fill: '#0d9488', border: '#0f766e' }, // Teal
  { color: '#06b6d4', fill: '#0891b2', border: '#0e7490' }, // Cyan
  { color: '#84cc16', fill: '#65a30d', border: '#4d7c0f' }, // Lime
  { color: '#f43f5e', fill: '#e11d48', border: '#be123c' }, // Rose
  { color: '#a855f7', fill: '#9333ea', border: '#7e22ce' }, // Purple
  { color: '#eab308', fill: '#ca8a04', border: '#a16207' }, // Yellow
];

/**
 * Deterministic hash function (djb2 algorithm).
 * Produces consistent numeric hash for any string input.
 */
export function hashString(str: string): number {
  let hash = 5381;
  for (let i = 0; i < str.length; i++) {
    hash = (hash * 33) ^ str.charCodeAt(i);
  }
  return hash >>> 0; // Convert to unsigned 32-bit integer
}

/**
 * Returns a CSS class name for a task group.
 * Same groupId always returns the same color class.
 *
 * @param groupId - The group identifier, or null for ungrouped tasks
 * @returns CSS class name: 'ungrouped' or 'group-{0-9}'
 */
export function getTaskGroupColorClass(groupId: string | null | undefined): string {
  if (groupId == null) {
    return 'ungrouped';
  }
  const index = hashString(groupId) % TASK_GROUP_PALETTE.length;
  return `group-${index}`;
}

/**
 * Returns the color object for a task group.
 * Useful for inline styles or dynamic color application.
 *
 * @param groupId - The group identifier, or null for ungrouped tasks
 * @returns Color object with color, fill, and border properties, or null for ungrouped
 */
export function getTaskGroupColor(groupId: string | null | undefined): TaskGroupColor | null {
  if (groupId == null) {
    return null;
  }
  const index = hashString(groupId) % TASK_GROUP_PALETTE.length;
  return TASK_GROUP_PALETTE[index];
}
