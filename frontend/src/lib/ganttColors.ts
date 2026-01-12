/**
 * Number of distinct group colors available in gantt.css.
 * CSS classes are: ungrouped, group-0 through group-9
 */
const GROUP_COLOR_COUNT = 10;

/**
 * Deterministic hash function (djb2 algorithm).
 * Produces consistent numeric hash for any string input.
 */
function hashString(str: string): number {
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
  return `group-${hashString(groupId) % GROUP_COLOR_COUNT}`;
}
