import type { ITask } from '@svar-ui/react-gantt';
import { Circle } from 'lucide-react';
import type { JsonValue, TaskStatus } from 'shared/types';
import type { SvarGanttTask } from '@/lib/transformGantt';
import { formatTokenCount } from '@/lib/utils';
import { statusLabels, statusBoardColors } from '@/utils/statusLabels';

interface GanttTooltipContentProps {
  data: ITask;
}

const STATUS_VALUES = new Set<string>(['todo', 'inprogress', 'inreview', 'done', 'cancelled']);

function isTaskStatus(value: string): value is TaskStatus {
  return STATUS_VALUES.has(value);
}

/**
 * Calculate duration between two dates, returning a human-readable string.
 * Formats as "X hours Y minutes" for durations >= 1 hour, or "X minutes" for shorter.
 */
function calculateDuration(start: Date, end: Date): string {
  const diffMs = end.getTime() - start.getTime();
  const totalMinutes = Math.round(diffMs / (1000 * 60));

  if (totalMinutes < 1) {
    return '< 1 minute';
  }

  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours === 0) {
    return `${minutes} minute${minutes !== 1 ? 's' : ''}`;
  }

  if (minutes === 0) {
    return `${hours} hour${hours !== 1 ? 's' : ''}`;
  }

  return `${hours} hour${hours !== 1 ? 's' : ''} ${minutes} minute${minutes !== 1 ? 's' : ''}`;
}

/**
 * Format a date as a human-readable date/time string.
 * Uses locale-aware formatting with medium date and short time.
 */
function formatDateTime(date: Date): string {
  return date.toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });
}

export function GanttTooltipContent({ data }: GanttTooltipContentProps) {
  if (!data) {
    return null;
  }

  const task = data as unknown as SvarGanttTask;
  const type = task.type;

  // Status display only makes sense when type is a TaskStatus (not a group color class)
  const showStatusInfo = isTaskStatus(type);
  const statusColor = showStatusInfo ? statusBoardColors[type] : undefined;
  const statusLabel = showStatusInfo ? statusLabels[type] : undefined;

  const hasTokens =
    task.totalInputTokens != null || task.totalOutputTokens != null;

  return (
    <div className="flex flex-col gap-2 p-3 min-w-[180px] bg-popover text-popover-foreground border rounded-md shadow-md">
      <div className="font-semibold text-sm">{task.text}</div>

      {showStatusInfo && statusColor && statusLabel && (
        <div className="flex items-center gap-1.5">
          <Circle
            className="h-2.5 w-2.5 fill-current"
            style={{ color: `var(${statusColor})` }}
          />
          <span className="text-xs">{statusLabel}</span>
        </div>
      )}

      <div className="flex flex-col gap-1 text-xs text-muted-foreground">
        <div>
          <span className="font-medium">Duration:</span>{' '}
          {calculateDuration(task.start, task.end)}
        </div>
        <div>
          <span className="font-medium">Start:</span>{' '}
          {formatDateTime(task.start)}
        </div>
        <div>
          <span className="font-medium">End:</span> {formatDateTime(task.end)}
        </div>
        {hasTokens && (
          <div className="border-t border-dashed mt-1 pt-1">
            <div className="flex items-center gap-1 mb-0.5">
              <span className="font-medium text-foreground">Tokens:</span>
              <span>
                {formatTokenCount(task.totalInputTokens) || '0'} /{' '}
                {formatTokenCount(task.totalOutputTokens) || '0'}
              </span>
            </div>
            {task.tokenUsageMetadata &&
              Array.isArray(task.tokenUsageMetadata) &&
              task.tokenUsageMetadata.length > 0 && (
                <div className="pl-2 flex flex-col gap-0.5 opacity-80 scale-[0.95] origin-left">
                  {Object.entries(
                    task.tokenUsageMetadata.reduce(
                      (acc: Record<string, number>, curr: JsonValue) => {
                        if (curr && typeof curr === 'object' && !Array.isArray(curr)) {
                          Object.entries(curr).forEach(([k, v]) => {
                            if (k === 'usage') return;
                            if (typeof v === 'number') {
                              acc[k] = (acc[k] || 0) + v;
                            }
                          });
                        }
                        return acc;
                      },
                      {}
                    )
                  ).map(([key, val]) => (
                    <div key={key} className="flex justify-between gap-4">
                      <span className="capitalize">
                        {key.replace(/_/g, ' ')}:
                      </span>
                      <span>{formatTokenCount(val)}</span>
                    </div>
                  ))}
                </div>
              )}
          </div>
        )}
      </div>
    </div>
  );
}
