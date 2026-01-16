import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, MessageCircle, Archive, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Loader } from '@/components/ui/loader';
import { useConversations, useDeleteConversation } from '@/hooks/useConversations';
import { ConfirmDialog } from '@/components/dialogs/shared/ConfirmDialog';
import type { ConversationSession } from 'shared/types';
import { cn, formatRelativeTime } from '@/lib/utils';

interface ConversationListProps {
  projectId: string;
  selectedConversationId?: string;
  onSelectConversation: (conversation: ConversationSession) => void;
  onCreateConversation: () => void;
}

export function ConversationList({
  projectId,
  selectedConversationId,
  onSelectConversation,
  onCreateConversation,
}: ConversationListProps) {
  const { t } = useTranslation('common');
  const { data: conversations, isLoading, error } = useConversations(projectId);
  const deleteConversation = useDeleteConversation();

  const sortedConversations = useMemo(() => {
    if (!conversations) return [];
    return [...conversations].sort(
      (a, b) =>
        new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
    );
  }, [conversations]);

  const handleDelete = async (
    e: React.MouseEvent,
    conversation: ConversationSession
  ) => {
    e.stopPropagation();

    const result = await ConfirmDialog.show({
      title: t('conversations.deleteTitle', {
        defaultValue: 'Delete Conversation',
      }),
      message: t('conversations.deleteMessage', {
        defaultValue: `Are you sure you want to delete "${conversation.title}"? This action cannot be undone.`,
        title: conversation.title,
      }),
      confirmText: t('common:buttons.delete', { defaultValue: 'Delete' }),
      cancelText: t('common:buttons.cancel', { defaultValue: 'Cancel' }),
      variant: 'destructive',
    });

    if (result === 'confirmed') {
      deleteConversation.mutate(conversation.id);
    }
  };

  if (isLoading) {
    return <Loader message={t('common:states.loading')} size={24} />;
  }

  if (error) {
    return (
      <div className="text-destructive p-4">
        {t('common:states.error')}: {error.message}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between p-3 border-b">
        <h3 className="text-sm font-medium">
          {t('conversations.title', { defaultValue: 'Conversations' })}
        </h3>
        <Button variant="icon" size="sm" onClick={onCreateConversation}>
          <Plus className="h-4 w-4" />
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {sortedConversations.length === 0 ? (
          <div className="p-4 text-center text-muted-foreground text-sm">
            {t('conversations.empty', {
              defaultValue: 'No conversations yet',
            })}
          </div>
        ) : (
          <div className="divide-y">
            {sortedConversations.map((conversation) => (
              <div
                key={conversation.id}
                className={cn(
                  'p-3 cursor-pointer hover:bg-accent transition-colors group',
                  selectedConversationId === conversation.id && 'bg-accent'
                )}
                onClick={() => onSelectConversation(conversation)}
              >
                <div className="flex items-start gap-2">
                  <div className="flex-shrink-0 mt-0.5">
                    {conversation.status === 'archived' ? (
                      <Archive className="h-4 w-4 text-muted-foreground" />
                    ) : (
                      <MessageCircle className="h-4 w-4 text-muted-foreground" />
                    )}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-medium truncate">
                        {conversation.title}
                      </span>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="opacity-0 group-hover:opacity-100 h-6 w-6 p-0"
                        onClick={(e) => handleDelete(e, conversation)}
                      >
                        <Trash2 className="h-3 w-3 text-muted-foreground hover:text-destructive" />
                      </Button>
                    </div>
                    <div className="flex items-center gap-2 mt-1">
                      <span className="text-xs text-muted-foreground">
                        {formatRelativeTime(conversation.updated_at)}
                      </span>
                      {conversation.executor && (
                        <span className="text-xs text-muted-foreground px-1.5 py-0.5 bg-muted rounded">
                          {conversation.executor}
                        </span>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export default ConversationList;
