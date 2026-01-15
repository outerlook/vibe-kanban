import { LayoutGrid, Layers } from 'lucide-react';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';

export type ViewMode = 'kanban' | 'groups';

interface ViewToggleProps {
  value: ViewMode;
  onChange: (mode: ViewMode) => void;
}

export function ViewToggle({ value, onChange }: ViewToggleProps) {
  return (
    <ToggleGroup
      type="single"
      value={value}
      onValueChange={(v) => {
        if (v) onChange(v as ViewMode);
      }}
      className="bg-muted rounded-md p-0.5"
    >
      <ToggleGroupItem
        value="kanban"
        active={value === 'kanban'}
        aria-label="Kanban view"
      >
        <LayoutGrid className="h-3 w-3" />
      </ToggleGroupItem>
      <ToggleGroupItem
        value="groups"
        active={value === 'groups'}
        aria-label="Groups view"
      >
        <Layers className="h-3 w-3" />
      </ToggleGroupItem>
    </ToggleGroup>
  );
}
