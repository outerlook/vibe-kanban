import { Check, AlertTriangle, GitBranch, Loader2 } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { useBranchAncestorStatus } from '@/hooks';
import { StatusCountBadge } from './StatusCountBadge';
import type { TaskGroupWithStats, TaskStatus } from 'shared/types';

interface GroupCardProps {
  group: TaskGroupWithStats;
  repoId: string;
  onClick?: () => void;
}

const statusOrder: TaskStatus[] = [
  'inprogress',
  'todo',
  'inreview',
  'done',
  'cancelled',
];

export function GroupCard({ group, repoId, onClick }: GroupCardProps) {
  const { data: branchStatus, isLoading: isBranchLoading } =
    useBranchAncestorStatus(repoId, group.base_branch ?? undefined);

  const counts = group.task_counts;

  return (
    <Card
      onClick={onClick}
      className={cn(
        'p-4 cursor-pointer transition-colors',
        'hover:bg-accent/50 border border-border rounded-lg',
        onClick && 'hover:shadow-sm'
      )}
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-start justify-between gap-2">
          <h3 className="font-medium text-base text-foreground truncate">
            {group.name}
          </h3>

          {group.base_branch && (
            <div className="flex items-center gap-1.5 shrink-0">
              {isBranchLoading ? (
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              ) : branchStatus?.is_ancestor ? (
                <Check className="h-4 w-4 text-emerald-500" />
              ) : (
                <AlertTriangle className="h-4 w-4 text-amber-500" />
              )}
            </div>
          )}
        </div>

        {group.base_branch && (
          <Badge
            variant="outline"
            className="w-fit text-xs font-normal gap-1 px-2 py-0.5"
          >
            <GitBranch className="h-3 w-3" />
            {group.base_branch}
          </Badge>
        )}

        <div className="flex flex-wrap gap-1.5">
          {statusOrder.map((status) => (
            <StatusCountBadge
              key={status}
              status={status}
              count={counts[status]}
            />
          ))}
        </div>
      </div>
    </Card>
  );
}
