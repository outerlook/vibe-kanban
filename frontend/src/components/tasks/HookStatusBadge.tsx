import { CheckCircle2, Loader2, MinusCircle, XCircle } from 'lucide-react';
import { useProjectTasksContext } from '@/contexts/ProjectTasksContext';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import type { HookExecutionStatus } from 'shared/types';
import type { LucideIcon } from 'lucide-react';

interface HookStatusBadgeProps {
  taskId: string;
}

interface BadgeConfig {
  icon: LucideIcon;
  className: string;
  iconClassName?: string;
}

const badgeConfigs: Record<HookExecutionStatus, BadgeConfig> = {
  running: {
    icon: Loader2,
    className: 'bg-muted text-muted-foreground',
    iconClassName: 'animate-spin',
  },
  failed: {
    icon: XCircle,
    className: 'bg-destructive/10 text-destructive',
  },
  skipped: {
    icon: MinusCircle,
    className: 'bg-muted text-muted-foreground',
  },
  completed: {
    icon: CheckCircle2,
    className: 'bg-green-500/10 text-green-600 dark:text-green-400',
  },
};

const statusPriority: HookExecutionStatus[] = ['running', 'failed', 'skipped', 'completed'];

export function HookStatusBadge({ taskId }: HookStatusBadgeProps) {
  const { hookExecutionsByTaskId } = useProjectTasksContext();
  const executions = hookExecutionsByTaskId[taskId];

  if (!executions || executions.length === 0) return null;

  const countByStatus = executions.reduce(
    (acc, e) => {
      acc[e.status] = (acc[e.status] || 0) + 1;
      return acc;
    },
    {} as Record<HookExecutionStatus, number>
  );

  const activeStatus = statusPriority.find((s) => countByStatus[s] > 0);
  if (!activeStatus) return null;

  const config = badgeConfigs[activeStatus];
  const Icon = config.icon;
  const count = countByStatus[activeStatus];
  const handlerNames = [...new Set(executions.map((e) => e.handler_name))].join(', ');

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${config.className}`}>
            <Icon className={`h-3.5 w-3.5 flex-shrink-0 ${config.iconClassName ?? ''}`} />
            <span>{count}</span>
          </span>
        </TooltipTrigger>
        <TooltipContent side="top" className="max-w-xs">
          <p className="text-sm">{handlerNames}</p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
