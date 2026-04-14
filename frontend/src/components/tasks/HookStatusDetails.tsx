import { useMemo } from 'react';
import {
  CheckCircle2,
  ExternalLink,
  Loader2,
  MinusCircle,
  XCircle,
} from 'lucide-react';
import { useProjectTasksContext } from '@/contexts/ProjectTasksContext';
import { useTaskWorkflowAssociations } from '@/hooks/useWorkflowAssociations';
import {
  getWorkflowAssociationScopeLabel,
} from '@/components/tasks/workflowAssociationHelpers';
import type { HookExecution, HookExecutionStatus, HookPoint } from 'shared/types';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface HookStatusDetailsProps {
  taskId: string;
}

const hookPointDisplayNames: Record<HookPoint, string> = {
  pre_task_create: 'Pre-Task Create',
  post_task_create: 'Post-Task Create',
  pre_task_status_change: 'Pre-Task Status Change',
  post_task_status_change: 'Post-Task Status Change',
  post_agent_complete: 'Post-Agent Complete',
  post_dependency_unblocked: 'Post-Dependency Unblocked',
};

function getHookPointDisplayName(hookPoint: HookPoint): string {
  return hookPointDisplayNames[hookPoint] ?? hookPoint;
}

function formatDuration(startedAt: string, completedAt: string): string {
  const start = new Date(startedAt).getTime();
  const end = new Date(completedAt).getTime();
  const durationMs = end - start;

  if (durationMs < 1000) {
    return `${durationMs}ms`;
  }
  if (durationMs < 60000) {
    return `${(durationMs / 1000).toFixed(1)}s`;
  }
  const minutes = Math.floor(durationMs / 60000);
  const seconds = Math.floor((durationMs % 60000) / 1000);
  return `${minutes}m ${seconds}s`;
}

function StatusIcon({ status }: { status: HookExecutionStatus }) {
  switch (status) {
    case 'running':
      return <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />;
    case 'completed':
      return <CheckCircle2 className="h-4 w-4 text-green-500" />;
    case 'failed':
      return <XCircle className="h-4 w-4 text-destructive" />;
    case 'skipped':
      return <MinusCircle className="h-4 w-4 text-muted-foreground" />;
    default:
      return null;
  }
}

function sortExecutions(executions: HookExecution[]): HookExecution[] {
  const statusPriority: Record<HookExecutionStatus, number> = {
    running: 0,
    failed: 1,
    skipped: 2,
    completed: 3,
  };

  return [...executions].sort((a, b) => {
    const priorityDiff = statusPriority[a.status] - statusPriority[b.status];
    if (priorityDiff !== 0) return priorityDiff;
    return new Date(b.started_at).getTime() - new Date(a.started_at).getTime();
  });
}

function HookExecutionItem({ execution }: { execution: HookExecution }) {
  const hookPoint = getHookPointDisplayName(execution.hook_point);
  const duration = execution.completed_at
    ? formatDuration(execution.started_at, execution.completed_at)
    : null;

  const itemContent = (
    <div className="flex items-center gap-3 py-2 px-1">
      <StatusIcon status={execution.status} />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium truncate">{hookPoint}</div>
        <div className="text-xs text-muted-foreground truncate">
          Internal handler: {execution.handler_name}
        </div>
      </div>
      {duration && (
        <div className="text-xs text-muted-foreground flex-shrink-0">
          {duration}
        </div>
      )}
    </div>
  );

  if (execution.error) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="cursor-help">{itemContent}</div>
          </TooltipTrigger>
          <TooltipContent side="left" className="max-w-xs">
            <p className="text-sm text-destructive break-words whitespace-pre-wrap">
              {execution.error}
            </p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return itemContent;
}

export function HookStatusDetails({ taskId }: HookStatusDetailsProps) {
  const { hookExecutionsByTaskId } = useProjectTasksContext();
  const { data: workflowResolution } = useTaskWorkflowAssociations(taskId);

  const sortedExecutions = useMemo(
    () => sortExecutions(hookExecutionsByTaskId[taskId] ?? []),
    [hookExecutionsByTaskId, taskId]
  );

  const associations = workflowResolution?.associations ?? [];

  if (associations.length === 0 && sortedExecutions.length === 0) {
    return (
      <div className="text-sm text-muted-foreground py-4 text-center">
        No associated workflows or hook executions
      </div>
    );
  }

  return (
    <div className="space-y-4 py-4">
      {associations.length > 0 && (
        <div className="space-y-2">
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Associated n8n workflows
          </div>
          <div className="space-y-2">
            {associations.map((association) => (
              <a
                key={`${association.scope}-${association.workflow_id}`}
                href={association.url}
                target="_blank"
                rel="noreferrer"
                className="flex items-center justify-between gap-3 rounded-md border px-3 py-2 hover:bg-accent/50"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium truncate">
                      {association.label}
                    </span>
                    {association.is_effective ? (
                      <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-primary">
                        Effective
                      </span>
                    ) : null}
                  </div>
                  <div className="text-xs text-muted-foreground truncate">
                    {getWorkflowAssociationScopeLabel(association.scope)} ·{' '}
                    {association.workflow_id}
                  </div>
                </div>
                <ExternalLink className="h-4 w-4 flex-shrink-0 text-muted-foreground" />
              </a>
            ))}
          </div>
        </div>
      )}

      {sortedExecutions.length > 0 && (
        <div className="space-y-1">
          <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Execution history
          </div>
          <div className="divide-y divide-border rounded-md border">
            {sortedExecutions.map((execution) => (
              <HookExecutionItem key={execution.id} execution={execution} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
