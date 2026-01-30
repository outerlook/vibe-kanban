import { CheckCircle2, Loader2, XCircle } from 'lucide-react';
import { useProjectTasksContext } from '@/contexts/ProjectTasksContext';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface HookStatusBadgeProps {
  taskId: string;
}

export function HookStatusBadge({ taskId }: HookStatusBadgeProps) {
  const { hookExecutionsByTaskId } = useProjectTasksContext();
  const executions = hookExecutionsByTaskId[taskId];

  if (!executions || executions.length === 0) return null;

  const runningCount = executions.filter((e) => e.status === 'running').length;
  const failedCount = executions.filter((e) => e.status === 'failed').length;
  const completedCount = executions.filter(
    (e) => e.status === 'completed'
  ).length;

  const handlerNames = [...new Set(executions.map((e) => e.handler_name))];
  const tooltipContent = handlerNames.join(', ');

  if (runningCount > 0) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin flex-shrink-0" />
              <span>{runningCount}</span>
            </span>
          </TooltipTrigger>
          <TooltipContent side="top" className="max-w-xs">
            <p className="text-sm">{tooltipContent}</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  if (failedCount > 0) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-destructive/10 text-destructive">
              <XCircle className="h-3.5 w-3.5 flex-shrink-0" />
              <span>{failedCount}</span>
            </span>
          </TooltipTrigger>
          <TooltipContent side="top" className="max-w-xs">
            <p className="text-sm">{tooltipContent}</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  if (completedCount > 0) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-green-500/10 text-green-600 dark:text-green-400">
              <CheckCircle2 className="h-3.5 w-3.5 flex-shrink-0" />
              <span>{completedCount}</span>
            </span>
          </TooltipTrigger>
          <TooltipContent side="top" className="max-w-xs">
            <p className="text-sm">{tooltipContent}</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return null;
}
