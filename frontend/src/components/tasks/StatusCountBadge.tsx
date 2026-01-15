import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { statusLabelsShort, statusBadgeStyles } from '@/utils/statusLabels';
import type { TaskStatus } from 'shared/types';

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
        statusBadgeStyles[status],
        className
      )}
    >
      {numCount} {statusLabelsShort[status]}
    </Badge>
  );
}
