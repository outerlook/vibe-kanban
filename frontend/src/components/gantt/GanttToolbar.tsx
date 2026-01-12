import { Button } from '@/components/ui/button';

interface GanttToolbarProps {
  colorMode: 'status' | 'group';
  onColorModeChange: (mode: 'status' | 'group') => void;
}

export function GanttToolbar({
  colorMode,
  onColorModeChange,
}: GanttToolbarProps) {
  return (
    <div className="flex items-center gap-2 px-4 py-2">
      <span className="text-sm text-muted-foreground">Color by:</span>
      <Button
        variant={colorMode === 'status' ? 'default' : 'outline'}
        size="sm"
        onClick={() => onColorModeChange('status')}
      >
        Status
      </Button>
      <Button
        variant={colorMode === 'group' ? 'default' : 'outline'}
        size="sm"
        onClick={() => onColorModeChange('group')}
      >
        Task Group
      </Button>
    </div>
  );
}
