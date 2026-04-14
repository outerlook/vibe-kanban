import { Workflow } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { useTaskWorkflowAssociations } from '@/hooks/useWorkflowAssociations';

interface WorkflowAssociationBadgeProps {
  taskId: string;
}

export function WorkflowAssociationBadge({
  taskId,
}: WorkflowAssociationBadgeProps) {
  const { data: workflowResolution } = useTaskWorkflowAssociations(taskId);
  const effective = workflowResolution?.effective;

  if (!effective) {
    return null;
  }

  return (
    <Badge variant="outline" className="w-fit text-sky-600 dark:text-sky-400">
      <Workflow className="mr-1 h-3 w-3" />
      {effective.label}
    </Badge>
  );
}
