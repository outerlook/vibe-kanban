import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { TaskStatus } from 'shared/types';

const statusStyles: Record<TaskStatus, string> = {
  todo: 'bg-muted text-muted-foreground',
  inprogress: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300',
  inreview: 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300',
  done: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300',
  cancelled: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300',
};

const statusLabelsShort: Record<TaskStatus, string> = {
  todo: 'To Do',
  inprogress: 'In Progress',
  inreview: 'Review',
  done: 'Done',
  cancelled: 'Cancelled',
};

interface StatusCountBadgeProps {
  status: TaskStatus;
  count: number | bigint;
  className?: string;
}

export function StatusCountBadge({
  status,
  count,
  className,
}: StatusCountBadgeProps) {
  const numCount = typeof count === 'bigint' ? Number(count) : count;

  if (numCount === 0) {
    return null;
  }

  return (
    <Badge
      variant="secondary"
      className={cn(
        'text-xs font-medium border-0 px-2 py-0.5',
        statusStyles[status],
        className
      )}
    >
      {numCount} {statusLabelsShort[status]}
    </Badge>
  );
}
