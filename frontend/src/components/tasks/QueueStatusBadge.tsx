import { Loader2, AlertTriangle } from 'lucide-react';
import { useQueueStatus } from '@/hooks';
import type { MergeQueueStatus } from 'shared/types';

interface QueueStatusBadgeProps {
  workspaceId: string;
}

const statusStyles: Record<
  Exclude<MergeQueueStatus, 'completed'>,
  { bg: string; text: string }
> = {
  queued: {
    bg: 'bg-sky-100/60 dark:bg-sky-900/30',
    text: 'text-sky-700 dark:text-sky-300',
  },
  merging: {
    bg: 'bg-muted',
    text: 'text-muted-foreground',
  },
  conflict: {
    bg: 'bg-destructive/10 dark:bg-destructive/20',
    text: 'text-destructive',
  },
};

export function QueueStatusBadge({ workspaceId }: QueueStatusBadgeProps) {
  const { data: queueEntry } = useQueueStatus(workspaceId);

  if (!queueEntry) return null;

  const status = queueEntry.status as MergeQueueStatus;
  if (status === 'completed') return null;

  const style = statusStyles[status];

  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${style.bg} ${style.text}`}
    >
      {status === 'merging' && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
      {status === 'conflict' && <AlertTriangle className="h-3.5 w-3.5" />}
      {status === 'queued' && 'Queued'}
      {status === 'merging' && 'Merging...'}
      {status === 'conflict' && 'Conflict'}
    </span>
  );
}
