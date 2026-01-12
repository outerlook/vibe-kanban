/**
 * SVAR Gantt zoom configuration
 * Defines scale configurations for different zoom levels
 */

export type GanttViewMode = 'Day' | 'Week' | 'Month';

export interface GanttScale {
  unit: 'day' | 'week' | 'month' | 'year';
  step: number;
  format: string;
}

export interface GanttZoomLevel {
  minCellWidth: number;
  maxCellWidth: number;
  scales: GanttScale[];
}

/**
 * Zoom configuration with 3 levels:
 * 0 - Day: day/month granularity
 * 1 - Week: week/month granularity
 * 2 - Month: month/year granularity
 */
export const GANTT_ZOOM_CONFIG: GanttZoomLevel[] = [
  // Level 0: Day view - shows days grouped by month
  {
    minCellWidth: 60,
    maxCellWidth: 120,
    scales: [
      { unit: 'month', step: 1, format: 'MMMM yyyy' },
      { unit: 'day', step: 1, format: 'd' },
    ],
  },
  // Level 1: Week view - shows weeks grouped by month
  {
    minCellWidth: 80,
    maxCellWidth: 160,
    scales: [
      { unit: 'month', step: 1, format: 'MMMM yyyy' },
      { unit: 'week', step: 1, format: "'W'w" },
    ],
  },
  // Level 2: Month view - shows months grouped by year
  {
    minCellWidth: 100,
    maxCellWidth: 200,
    scales: [
      { unit: 'year', step: 1, format: 'yyyy' },
      { unit: 'month', step: 1, format: 'MMM' },
    ],
  },
];

const VIEW_MODE_TO_ZOOM: Record<GanttViewMode, number> = {
  Day: 0,
  Week: 1,
  Month: 2,
};

/**
 * Maps a GanttViewMode to its corresponding zoom level index
 */
export function viewModeToZoomLevel(mode: GanttViewMode): number {
  return VIEW_MODE_TO_ZOOM[mode];
}
