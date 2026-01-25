import { Loader2 } from 'lucide-react';
import { useQueueStatus } from '@/hooks';
import type { MergeQueueStatus } from 'shared/types';

interface QueueStatusBadgeProps {
  workspaceId: string;
}

const statusStyles: Record<
  'queued' | 'merging',
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
};

export function QueueStatusBadge({ workspaceId }: QueueStatusBadgeProps) {
  const { data: queueEntry } = useQueueStatus(workspaceId);

  if (!queueEntry) return null;

  const status = queueEntry.status as MergeQueueStatus;
  const style = statusStyles[status];

  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${style.bg} ${style.text}`}
    >
      {status === 'merging' && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
      {status === 'queued' && 'Queued'}
      {status === 'merging' && 'Merging...'}
    </span>
  );
}
