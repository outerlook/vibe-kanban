import { Layers } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { getTaskGroupColorClass } from '@/lib/ganttColors';

const BADGE_COLORS: Record<string, string> = {
  'group-0': 'bg-indigo-100 text-indigo-700 dark:bg-indigo-900 dark:text-indigo-200',
  'group-1': 'bg-violet-100 text-violet-700 dark:bg-violet-900 dark:text-violet-200',
  'group-2': 'bg-pink-100 text-pink-700 dark:bg-pink-900 dark:text-pink-200',
  'group-3': 'bg-rose-100 text-rose-700 dark:bg-rose-900 dark:text-rose-200',
  'group-4': 'bg-orange-100 text-orange-700 dark:bg-orange-900 dark:text-orange-200',
  'group-5': 'bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-200',
  'group-6': 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-200',
  'group-7': 'bg-teal-100 text-teal-700 dark:bg-teal-900 dark:text-teal-200',
  'group-8': 'bg-cyan-100 text-cyan-700 dark:bg-cyan-900 dark:text-cyan-200',
  'group-9': 'bg-sky-100 text-sky-700 dark:bg-sky-900 dark:text-sky-200',
};

interface TaskGroupBadgeProps {
  groupId: string | null | undefined;
  groupName: string | null | undefined;
  className?: string;
  onClick?: (e: React.MouseEvent) => void;
  onContextMenu?: (e: React.MouseEvent) => void;
}

export function TaskGroupBadge({
  groupId,
  groupName,
  className,
  onClick,
  onContextMenu,
}: TaskGroupBadgeProps) {
  if (!groupId || !groupName) {
    return null;
  }

  const colorClass = getTaskGroupColorClass(groupId);
  const badgeColorClasses = BADGE_COLORS[colorClass] ?? '';

  return (
    <Badge
      variant={badgeColorClasses ? undefined : 'secondary'}
      className={cn(
        'text-xs font-normal gap-1 py-0.5 px-2',
        badgeColorClasses,
        onClick && 'cursor-pointer hover:opacity-80',
        className
      )}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      <Layers className="h-3 w-3" />
      {groupName}
    </Badge>
  );
}
