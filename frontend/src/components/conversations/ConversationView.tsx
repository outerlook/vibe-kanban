import { useTranslation } from 'react-i18next';
import { Loader } from '@/components/ui/loader';
import { useConversation } from '@/hooks/useConversations';
import VirtualizedList from '@/components/logs/VirtualizedList';
import { EntriesProvider } from '@/contexts/EntriesContext';

interface ConversationViewProps {
  conversationId: string;
}

export function ConversationView({ conversationId }: ConversationViewProps) {
  const { t } = useTranslation('common');
  const { data: conversation, isLoading, error } = useConversation(conversationId);

  if (isLoading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <Loader message={t('common:states.loading')} />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center text-destructive">
        {t('common:states.error')}: {error.message}
      </div>
    );
  }

  if (!conversation) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted-foreground">
        {t('conversations.selectConversation', {
          defaultValue: 'Select a conversation',
        })}
      </div>
    );
  }

  return (
    <EntriesProvider key={conversationId}>
      <VirtualizedList
        key={conversationId}
        mode={{ type: 'conversation', conversationSessionId: conversationId }}
      />
    </EntriesProvider>
  );
}

export default ConversationView;
