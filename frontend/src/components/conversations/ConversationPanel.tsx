import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { MessageSquare, Pencil } from 'lucide-react';
import { ConversationList } from './ConversationList';
import { ConversationView } from './ConversationView';
import { MessageInput } from './MessageInput';
import { NewConversationDialog } from '@/components/dialogs/conversations/NewConversationDialog';
import { RenameConversationDialog } from '@/components/dialogs/conversations/RenameConversationDialog';
import {
  useSendMessage,
  useConversationExecutions,
  useStopConversationExecution,
} from '@/hooks/useConversations';
import type { ConversationSession, ExecutionProcessStatus } from 'shared/types';

interface ConversationPanelProps {
  projectId: string;
}

export function ConversationPanel({ projectId }: ConversationPanelProps) {
  const { t } = useTranslation('common');
  const [selectedConversation, setSelectedConversation] =
    useState<ConversationSession | null>(null);

  const sendMessage = useSendMessage();
  const { data: executions } = useConversationExecutions(
    selectedConversation?.id
  );

  // Check if there's an active execution (agent is responding)
  const runningExecutionId = executions?.find(
    (ep) => ep.status === ('running' as ExecutionProcessStatus)
  )?.id;

  const { stopExecution, isStopping } = useStopConversationExecution(
    selectedConversation?.id
  );

  const handleSelectConversation = useCallback(
    (conversation: ConversationSession) => {
      setSelectedConversation(conversation);
    },
    []
  );

  const handleCreateConversation = useCallback(() => {
    NewConversationDialog.show({ projectId }).then((result) => {
      if (result) {
        setSelectedConversation(result);
      }
    });
  }, [projectId]);

  const handleSendMessage = useCallback(
    (content: string) => {
      if (!selectedConversation) return;

      sendMessage.mutate({
        conversationId: selectedConversation.id,
        data: { content },
      });
    },
    [selectedConversation, sendMessage]
  );

  const handleRenameConversation = useCallback(() => {
    if (!selectedConversation) return;

    RenameConversationDialog.show({
      conversationId: selectedConversation.id,
      currentTitle: selectedConversation.title,
    }).then((result) => {
      if (result.action === 'confirmed' && result.title) {
        setSelectedConversation((prev) =>
          prev ? { ...prev, title: result.title! } : null
        );
      }
    });
  }, [selectedConversation]);

  return (
    <div className="flex h-full border rounded-lg overflow-hidden bg-background">
      {/* Sidebar with conversation list */}
      <div className="w-64 border-r flex-shrink-0 flex flex-col">
        <ConversationList
          projectId={projectId}
          selectedConversationId={selectedConversation?.id}
          onSelectConversation={handleSelectConversation}
          onCreateConversation={handleCreateConversation}
        />
      </div>

      {/* Main content area */}
      <div className="flex-1 flex flex-col min-w-0">
        {selectedConversation ? (
          <>
            {/* Header */}
            <div className="border-b px-4 py-3 flex items-center gap-2">
              <MessageSquare className="h-4 w-4 text-muted-foreground" />
              <h2 className="font-medium truncate">
                {selectedConversation.title}
              </h2>
              <button
                onClick={handleRenameConversation}
                className="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
                aria-label="Rename conversation"
              >
                <Pencil className="h-3.5 w-3.5" />
              </button>
              {selectedConversation.executor && (
                <span className="text-xs text-muted-foreground px-2 py-0.5 bg-muted rounded ml-auto">
                  {selectedConversation.executor}
                </span>
              )}
            </div>

            {/* Conversation view */}
            <ConversationView conversationId={selectedConversation.id} />

            {/* Message input */}
            <MessageInput
              onSend={handleSendMessage}
              disabled={!!runningExecutionId || sendMessage.isPending}
              onStop={() => runningExecutionId && stopExecution(runningExecutionId)}
              isStopping={isStopping}
              showStopButton={!!runningExecutionId}
            />
          </>
        ) : (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground">
            <MessageSquare className="h-12 w-12 mb-4 opacity-50" />
            <p>
              {t('conversations.selectOrCreate', {
                defaultValue: 'Select a conversation or create a new one',
              })}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

export default ConversationPanel;
