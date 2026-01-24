import { AlertCircle, Loader2 } from 'lucide-react';
import { useProjectTasksContext } from '@/contexts/ProjectTasksContext';
import type { OperationStatusType } from 'shared/types';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface OperationStatusBadgeProps {
  taskId: string;
}

const operationLabels: Record<OperationStatusType, string> = {
  generating_commit: 'Generating...',
  rebasing: 'Rebasing...',
  pushing: 'Pushing...',
  merging: 'Merging...',
};

export function OperationStatusBadge({ taskId }: OperationStatusBadgeProps) {
  const { operationStatusesByTaskId } = useProjectTasksContext();
  const status = operationStatusesByTaskId[taskId];

  if (!status) return null;

  const hasError = !!status.error;
  const label = operationLabels[status.operation_type];

  if (hasError) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-destructive/10 text-destructive max-w-[120px]">
              <AlertCircle className="h-3.5 w-3.5 flex-shrink-0" />
              <span className="truncate">Error</span>
            </span>
          </TooltipTrigger>
          <TooltipContent side="top" className="max-w-xs">
            <p className="text-sm">{status.error}</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return (
    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground max-w-[120px]">
      <Loader2 className="h-3.5 w-3.5 animate-spin flex-shrink-0" />
      <span className="truncate">{label}</span>
    </span>
  );
}
