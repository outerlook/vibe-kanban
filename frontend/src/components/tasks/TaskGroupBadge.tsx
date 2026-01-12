import { Layers } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

interface TaskGroupBadgeProps {
  groupId: string | null | undefined;
  groupName: string | null | undefined;
  className?: string;
  onClick?: (e: React.MouseEvent) => void;
}

export function TaskGroupBadge({
  groupId,
  groupName,
  className,
  onClick,
}: TaskGroupBadgeProps) {
  if (!groupId || !groupName) {
    return null;
  }

  return (
    <Badge
      variant="secondary"
      className={cn(
        'text-xs font-normal gap-1 py-0.5 px-2',
        onClick && 'cursor-pointer hover:bg-secondary/80',
        className
      )}
      onClick={onClick}
    >
      <Layers className="h-3 w-3" />
      {groupName}
    </Badge>
  );
}
