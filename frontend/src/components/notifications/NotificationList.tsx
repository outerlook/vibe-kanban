import { CheckCheck, Inbox } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { NotificationItem } from './NotificationItem';
import type { Notification } from 'shared/types';

interface NotificationListProps {
  notifications: Notification[];
  onMarkRead: (id: string) => void;
  onMarkAllRead: () => void;
  onDelete: (id: string) => void;
  onClose?: () => void;
}

export function NotificationList({
  notifications,
  onMarkRead,
  onMarkAllRead,
  onDelete,
  onClose,
}: NotificationListProps) {
  const hasUnread = notifications.some((n) => !n.is_read);

  if (notifications.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-8 px-4 text-muted-foreground">
        <Inbox className="h-8 w-8 mb-2" />
        <p className="text-sm">No notifications</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col">
      <div className="flex items-center justify-between px-3 py-2 border-b">
        <span className="text-sm font-medium">Notifications</span>
        {hasUnread && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-xs"
            onClick={onMarkAllRead}
          >
            <CheckCheck className="h-3 w-3 mr-1" />
            Mark all read
          </Button>
        )}
      </div>

      <div className="max-h-[400px] overflow-y-auto">
        {notifications.map((notification) => (
          <div key={notification.id} className="group">
            <NotificationItem
              notification={notification}
              onMarkRead={onMarkRead}
              onDelete={onDelete}
              onClose={onClose}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
