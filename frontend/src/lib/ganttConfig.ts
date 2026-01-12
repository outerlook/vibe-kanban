/**
 * SVAR Gantt scale configuration
 * Fixed minute-level view for task visibility
 *
 * SVAR format syntax:
 * %H = hour (24h), %i = minute, %M = abbreviated month, %j = day of month
 */
export const GANTT_SCALES = [
  { unit: 'day', step: 1, format: '%M %j' },
  { unit: 'hour', step: 1, format: '%H:%i' },
];
