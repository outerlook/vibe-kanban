import { Circle } from 'lucide-react';
import { useNotifications } from '@/hooks/useNotifications';
import { cn } from '@/lib/utils';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

interface ProjectNotificationBadgeProps {
  projectId: string;
  className?: string;
  showCount?: boolean;
}

export function ProjectNotificationBadge({
  projectId,
  className,
  showCount = false,
}: ProjectNotificationBadgeProps) {
  const { unreadCount } = useNotifications(projectId);

  if (unreadCount === 0) {
    return null;
  }

  const badge = showCount ? (
    <span
      className={cn(
        'inline-flex items-center justify-center',
        'min-w-[18px] h-[18px] px-1.5 rounded-full',
        'bg-primary text-primary-foreground text-[10px] font-medium',
        className
      )}
    >
      {unreadCount > 99 ? '99+' : unreadCount}
    </span>
  ) : (
    <Circle
      className={cn(
        'h-2.5 w-2.5 fill-amber-500 text-amber-500 shrink-0',
        className
      )}
    />
  );

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="inline-flex">{badge}</span>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          {unreadCount} unread notification{unreadCount !== 1 ? 's' : ''}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
