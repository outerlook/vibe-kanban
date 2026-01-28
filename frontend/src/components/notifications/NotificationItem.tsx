import { useNavigate } from 'react-router-dom';
import {
  CheckCircle2,
  AlertTriangle,
  HelpCircle,
  XCircle,
  MessageSquare,
  Trash2,
} from 'lucide-react';
import { cn, formatRelativeTime } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { paths } from '@/lib/paths';
import type { Notification, NotificationType, JsonValue } from 'shared/types';

interface NotificationItemProps {
  notification: Notification;
  onMarkRead: (id: string) => void;
  onDelete: (id: string) => void;
  onClose?: () => void;
}

const NOTIFICATION_ICONS: Record<NotificationType, React.ElementType> = {
  agent_complete: CheckCircle2,
  agent_approval_needed: AlertTriangle,
  agent_question_needed: HelpCircle,
  agent_error: XCircle,
  conversation_response: MessageSquare,
};

const NOTIFICATION_ICON_COLORS: Record<NotificationType, string> = {
  agent_complete: 'text-green-500',
  agent_approval_needed: 'text-amber-500',
  agent_question_needed: 'text-purple-500',
  agent_error: 'text-red-500',
  conversation_response: 'text-blue-500',
};

interface NotificationMetadata {
  task_id?: string;
  conversation_session_id?: string;
  [key: string]: JsonValue | undefined;
}

function parseMetadata(metadata: JsonValue | null): NotificationMetadata {
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)) {
    return {};
  }
  return metadata as NotificationMetadata;
}

export function NotificationItem({
  notification,
  onMarkRead,
  onDelete,
  onClose,
}: NotificationItemProps) {
  const navigate = useNavigate();
  const Icon = NOTIFICATION_ICONS[notification.notification_type];
  const iconColor = NOTIFICATION_ICON_COLORS[notification.notification_type];

  const handleClick = () => {
    if (!notification.is_read) {
      onMarkRead(notification.id);
    }

    // Navigate based on notification context
    const metadata = parseMetadata(notification.metadata);
    const projectId = notification.project_id;
    const taskId = metadata.task_id;
    const workspaceId = notification.workspace_id;
    const conversationSessionId = metadata.conversation_session_id;

    // Handle conversation_response notifications
    if (
      notification.notification_type === 'conversation_response' &&
      projectId &&
      conversationSessionId
    ) {
      navigate(paths.conversation(projectId, conversationSessionId));
    } else if (projectId && taskId && workspaceId) {
      navigate(paths.attempt(projectId, taskId, workspaceId));
    } else if (projectId && taskId) {
      navigate(paths.task(projectId, taskId));
    } else if (projectId) {
      navigate(paths.projectTasks(projectId));
    }

    onClose?.();
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDelete(notification.id);
  };

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          handleClick();
        }
      }}
      className={cn(
        'flex items-start gap-3 p-3 cursor-pointer hover:bg-muted/50 transition-colors',
        'border-b last:border-b-0',
        !notification.is_read && 'bg-muted/30'
      )}
    >
      <div className={cn('mt-0.5 shrink-0', iconColor)}>
        <Icon className="h-4 w-4" />
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-start justify-between gap-2">
          <p
            className={cn(
              'text-sm truncate',
              !notification.is_read && 'font-medium'
            )}
          >
            {notification.title}
          </p>
          <span className="text-xs text-muted-foreground shrink-0">
            {formatRelativeTime(notification.created_at)}
          </span>
        </div>
        <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
          {notification.message}
        </p>
      </div>

      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 hover:opacity-100"
        onClick={handleDelete}
        aria-label="Delete notification"
      >
        <Trash2 className="h-3 w-3" />
      </Button>

      {!notification.is_read && (
        <div className="h-2 w-2 rounded-full bg-primary shrink-0 mt-1.5" />
      )}
    </div>
  );
}
