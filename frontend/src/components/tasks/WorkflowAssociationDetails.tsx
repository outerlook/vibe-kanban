import { ExternalLink } from 'lucide-react';
import { useTaskWorkflowAssociations } from '@/hooks/useWorkflowAssociations';
import { getWorkflowAssociationScopeLabel } from '@/components/tasks/workflowAssociationHelpers';

interface WorkflowAssociationDetailsProps {
  taskId: string;
}

export function WorkflowAssociationDetails({
  taskId,
}: WorkflowAssociationDetailsProps) {
  const { data: workflowResolution } = useTaskWorkflowAssociations(taskId);
  const associations = workflowResolution?.associations ?? [];

  if (associations.length === 0) {
    return (
      <div className="py-4 text-center text-sm text-muted-foreground">
        No associated workflows
      </div>
    );
  }

  return (
    <div className="space-y-4 py-4">
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
                  <span className="truncate text-sm font-medium">
                    {association.label}
                  </span>
                  {association.is_effective ? (
                    <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-primary">
                      Effective
                    </span>
                  ) : null}
                </div>
                <div className="truncate text-xs text-muted-foreground">
                  {getWorkflowAssociationScopeLabel(association.scope)} ·{' '}
                  {association.workflow_id}
                </div>
              </div>
              <ExternalLink className="h-4 w-4 flex-shrink-0 text-muted-foreground" />
            </a>
          ))}
        </div>
      </div>
    </div>
  );
}
