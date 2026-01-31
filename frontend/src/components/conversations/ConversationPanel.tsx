import { useState, useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { MessageSquare, Pencil, GitBranch, Home } from 'lucide-react';
import { ConversationList } from './ConversationList';
import { ConversationView } from './ConversationView';
import { MessageInput } from './MessageInput';
import { NewConversationDialog } from '@/components/dialogs/conversations/NewConversationDialog';
import { RenameConversationDialog } from '@/components/dialogs/conversations/RenameConversationDialog';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  useSendMessage,
  useConversationExecutions,
  useStopConversationExecution,
} from '@/hooks/useConversations';
import { useConversationQueueStatus } from '@/hooks/useConversationQueueStatus';
import type { ConversationSession, ExecutionProcessStatus } from 'shared/types';

interface ConversationPanelProps {
  projectId: string;
  initialConversationId?: string;
}

export function ConversationPanel({ projectId, initialConversationId: _initialConversationId }: ConversationPanelProps) {
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
  const isExecutionRunning = !!runningExecutionId;

  const { stopExecution, isStopping } = useStopConversationExecution(
    selectedConversation?.id
  );

  // Queue status for queuing messages while agent is running
  const {
    isQueued,
    queuedMessage,
    isLoading: isQueueLoading,
    queueMessage,
    cancelQueue,
    refresh: refreshQueueStatus,
  } = useConversationQueueStatus(selectedConversation?.id);

  // Track previous execution count to detect new executions
  const prevExecutionCountRef = useRef(executions?.length ?? 0);

  // Refresh queue status when execution stops OR when a new execution starts
  useEffect(() => {
    const currentCount = executions?.length ?? 0;
    const prevCount = prevExecutionCountRef.current;
    prevExecutionCountRef.current = currentCount;

    if (!selectedConversation?.id) return;

    // Refresh when execution stops
    if (!isExecutionRunning) {
      refreshQueueStatus();
      return;
    }

    // Refresh when a new execution starts (could be queued message consumption)
    if (currentCount > prevCount) {
      refreshQueueStatus();
    }
  }, [
    isExecutionRunning,
    selectedConversation?.id,
    executions?.length,
    refreshQueueStatus,
  ]);

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

              <TooltipProvider>
                {selectedConversation.worktree_path ? (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Badge variant="outline" className="text-xs gap-1">
                        <GitBranch className="h-3 w-3" />
                        {selectedConversation.worktree_branch}
                      </Badge>
                    </TooltipTrigger>
                    <TooltipContent>
                      {selectedConversation.worktree_path}
                    </TooltipContent>
                  </Tooltip>
                ) : (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Badge variant="secondary" className="text-xs gap-1">
                        <Home className="h-3 w-3" />
                        main
                      </Badge>
                    </TooltipTrigger>
                    <TooltipContent>Main repository</TooltipContent>
                  </Tooltip>
                )}
              </TooltipProvider>

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
              conversationId={selectedConversation.id}
              onSend={handleSendMessage}
              isExecutionRunning={isExecutionRunning || sendMessage.isPending}
              onStop={() => runningExecutionId && stopExecution(runningExecutionId)}
              isStopping={isStopping}
              isQueued={isQueued}
              queuedMessage={queuedMessage}
              queueMessage={queueMessage}
              cancelQueue={cancelQueue}
              isQueueLoading={isQueueLoading}
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
