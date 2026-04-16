import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Text } from '@/components/ui/text';
import type { WorkflowAssociationFormState } from './workflowAssociationHelpers';

interface WorkflowAssociationFieldsProps {
  title: string;
  description: string;
  value: WorkflowAssociationFormState;
  onChange: (updates: Partial<WorkflowAssociationFormState>) => void;
  error?: string | null;
  onClear?: () => void;
}

export function WorkflowAssociationFields({
  title,
  description,
  value,
  onChange,
  error,
  onClear,
}: WorkflowAssociationFieldsProps) {
  return (
    <div className="space-y-4 rounded-lg border p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <div className="text-sm font-medium">{title}</div>
          <Text variant="secondary" size="sm" as="p">
            {description}
          </Text>
        </div>
        {onClear && (
          <Button type="button" variant="outline" size="sm" onClick={onClear}>
            Clear
          </Button>
        )}
      </div>

      <div className="grid gap-4 md:grid-cols-3">
        <div className="space-y-2">
          <Label htmlFor={`${title}-workflow-id`}>Workflow ID</Label>
          <Input
            id={`${title}-workflow-id`}
            value={value.workflow_id}
            onChange={(event) => onChange({ workflow_id: event.target.value })}
            placeholder="e.g. 8a1d2c4f"
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor={`${title}-workflow-label`}>Display name</Label>
          <Input
            id={`${title}-workflow-label`}
            value={value.label}
            onChange={(event) => onChange({ label: event.target.value })}
            placeholder="e.g. Review Attention"
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor={`${title}-workflow-url`}>Workflow URL</Label>
          <Input
            id={`${title}-workflow-url`}
            value={value.url}
            onChange={(event) => onChange({ url: event.target.value })}
            placeholder="https://workflows.example/review-attention"
          />
        </div>
      </div>

      {error ? <div className="text-sm text-destructive">{error}</div> : null}
    </div>
  );
}
