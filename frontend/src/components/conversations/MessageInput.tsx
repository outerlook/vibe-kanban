import { useState, useCallback, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Send, StopCircle, Loader2, Clock, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { imagesApi } from '@/lib/api';
import { cn } from '@/lib/utils';
import { useScratch } from '@/hooks/useScratch';
import { useDebouncedCallback } from '@/hooks/useDebouncedCallback';
import type { ScratchType, DraftFollowUpData, QueuedMessage } from 'shared/types';

interface MessageInputProps {
  conversationId: string;
  onSend: (content: string) => void;
  isExecutionRunning: boolean;
  onStop?: () => void;
  isStopping?: boolean;
  // Queue props
  isQueued: boolean;
  queuedMessage: QueuedMessage | null;
  queueMessage: (message: string, variant: string | null) => Promise<void>;
  cancelQueue: () => Promise<void>;
  isQueueLoading: boolean;
  placeholder?: string;
}

export function MessageInput({
  conversationId,
  onSend,
  isExecutionRunning,
  onStop,
  isStopping = false,
  isQueued,
  queuedMessage,
  queueMessage,
  cancelQueue,
  isQueueLoading,
  placeholder,
}: MessageInputProps) {
  const { t } = useTranslation('common');

  // Scratch integration for draft persistence
  const {
    scratch,
    updateScratch,
    isLoading: isScratchLoading,
  } = useScratch('DRAFT_CONVERSATION_MESSAGE' as ScratchType, conversationId);

  // Derive draft data from scratch
  const scratchData: DraftFollowUpData | undefined =
    scratch?.payload?.type === 'DRAFT_CONVERSATION_MESSAGE'
      ? scratch.payload.data
      : undefined;

  // Track whether the editor is focused
  const [isEditorFocused, setIsEditorFocused] = useState(false);

  // Local message state for immediate UI feedback (before debounced save)
  const [localMessage, setLocalMessage] = useState('');

  // Ref for scratch to avoid callback invalidation
  const scratchRef = useRef(scratch);
  useEffect(() => {
    scratchRef.current = scratch;
  }, [scratch]);

  // Save scratch helper
  const saveToScratch = useCallback(
    async (message: string) => {
      if (!conversationId) return;
      // Don't create empty scratch entries unless scratch already exists
      if (!message.trim() && !scratchRef.current) return;
      try {
        await updateScratch({
          payload: {
            type: 'DRAFT_CONVERSATION_MESSAGE',
            data: { message, variant: null },
          },
        });
      } catch (e) {
        console.error('Failed to save conversation draft', e);
      }
    },
    [conversationId, updateScratch]
  );

  // Debounced save for message changes (500ms)
  const { debounced: debouncedSave, cancel: cancelDebouncedSave } =
    useDebouncedCallback(
      useCallback(
        (value: string) => saveToScratch(value),
        [saveToScratch]
      ),
      500
    );

  // Sync local message from scratch when it loads (but not while user is typing)
  useEffect(() => {
    if (isScratchLoading) return;
    if (isEditorFocused) return; // Don't overwrite while user is typing
    setLocalMessage(scratchData?.message ?? '');
  }, [isScratchLoading, scratchData?.message, isEditorFocused]);

  // Refs for queue state to use in stable onChange handler
  const isQueuedRef = useRef(isQueued);
  useEffect(() => {
    isQueuedRef.current = isQueued;
  }, [isQueued]);

  const cancelQueueRef = useRef(cancelQueue);
  useEffect(() => {
    cancelQueueRef.current = cancelQueue;
  }, [cancelQueue]);

  const queuedMessageRef = useRef(queuedMessage);
  useEffect(() => {
    queuedMessageRef.current = queuedMessage;
  }, [queuedMessage]);

  // Ref to access debouncedSave without adding it as a dependency
  const debouncedSaveRef = useRef(debouncedSave);
  useEffect(() => {
    debouncedSaveRef.current = debouncedSave;
  }, [debouncedSave]);

  // When queued, display the queued message content so user can edit it
  const displayMessage =
    isQueued && queuedMessage ? queuedMessage.data.message : localMessage;

  const defaultPlaceholder = t('conversations.messagePlaceholder', {
    defaultValue: 'Type a message...',
  });

  // Stable onChange handler for WYSIWYGEditor
  const handleEditorChange = useCallback((value: string) => {
    // Auto-cancel queue when user starts editing
    if (isQueuedRef.current) {
      cancelQueueRef.current();
    }
    setLocalMessage(value); // Immediate update for UI responsiveness
    debouncedSaveRef.current(value); // Debounced save to scratch
  }, []);

  const handleSubmit = useCallback(() => {
    const trimmed = localMessage.trim();
    if (!trimmed) return;

    cancelDebouncedSave(); // Cancel any pending save
    onSend(trimmed);
    setLocalMessage('');
    // Scratch will be cleared on successful send by parent
  }, [localMessage, onSend, cancelDebouncedSave]);

  // Handler to queue the current message
  const handleQueueMessage = useCallback(async () => {
    if (!localMessage.trim()) return;

    // Cancel any pending debounced save and save immediately before queueing
    cancelDebouncedSave();
    await saveToScratch(localMessage);

    await queueMessage(localMessage, null);
  }, [localMessage, queueMessage, cancelDebouncedSave, saveToScratch]);

  // Keyboard shortcut handler - send or queue depending on state
  const handleCmdEnter = useCallback(() => {
    if (isExecutionRunning) {
      // When running, CMD+Enter queues the message (if not already queued)
      if (!isQueued) {
        handleQueueMessage();
      }
    } else {
      handleSubmit();
    }
  }, [isExecutionRunning, isQueued, handleQueueMessage, handleSubmit]);

  const handlePasteFiles = useCallback(
    async (files: File[]) => {
      if (!conversationId) return;

      for (const file of files) {
        try {
          const response = await imagesApi.uploadForConversation(
            conversationId,
            file
          );
          const imageMarkdown = `![${response.original_name}](${response.file_path})`;

          // If queued, cancel queue and use queued message as base
          if (isQueuedRef.current && queuedMessageRef.current) {
            cancelQueueRef.current();
            const base = queuedMessageRef.current.data.message;
            const newMessage = base
              ? `${base}\n\n${imageMarkdown}`
              : imageMarkdown;
            setLocalMessage(newMessage);
            debouncedSaveRef.current(newMessage);
          } else {
            setLocalMessage((prev) => {
              const newMessage = prev
                ? `${prev}\n\n${imageMarkdown}`
                : imageMarkdown;
              debouncedSaveRef.current(newMessage);
              return newMessage;
            });
          }
        } catch (error) {
          console.error('Failed to upload image:', error);
        }
      }
    },
    [conversationId]
  );

  return (
    <div className="border-t bg-background p-4">
      <div className="max-w-3xl mx-auto">
        {/* Queued message indicator */}
        {isQueued && queuedMessage && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground bg-muted p-3 rounded-md border mb-3">
            <Clock className="h-4 w-4 flex-shrink-0" />
            <div className="font-medium">
              {t(
                'conversations.queuedMessage',
                'Message queued - will execute when current run finishes'
              )}
            </div>
          </div>
        )}

        <div
          className="flex flex-col gap-2"
          onFocus={() => setIsEditorFocused(true)}
          onBlur={(e) => {
            // Only blur if focus is leaving the container entirely
            if (!e.currentTarget.contains(e.relatedTarget)) {
              setIsEditorFocused(false);
            }
          }}
        >
          <div className={cn('flex-1 border rounded-md px-3 py-2 bg-background')}>
            <WYSIWYGEditor
              placeholder={placeholder ?? defaultPlaceholder}
              value={displayMessage}
              onChange={handleEditorChange}
              disabled={false} // Never disabled - always allow typing
              onPasteFiles={handlePasteFiles}
              onCmdEnter={handleCmdEnter}
              className="min-h-[28px]"
              conversationId={conversationId}
            />
          </div>

          <div className="flex items-center justify-end gap-2">
            {isExecutionRunning ? (
              <>
                {/* Queue/Cancel Queue button when running */}
                {isQueued ? (
                  <Button
                    onClick={cancelQueue}
                    disabled={isQueueLoading}
                    size="sm"
                    variant="outline"
                  >
                    {isQueueLoading ? (
                      <Loader2 className="animate-spin h-4 w-4 mr-2" />
                    ) : (
                      <>
                        <X className="h-4 w-4 mr-2" />
                        {t('conversations.cancelQueue', 'Cancel Queue')}
                      </>
                    )}
                  </Button>
                ) : (
                  <Button
                    onClick={handleQueueMessage}
                    disabled={isQueueLoading || !localMessage.trim()}
                    size="sm"
                  >
                    {isQueueLoading ? (
                      <Loader2 className="animate-spin h-4 w-4 mr-2" />
                    ) : (
                      <>
                        <Clock className="h-4 w-4 mr-2" />
                        {t('conversations.queue', 'Queue')}
                      </>
                    )}
                  </Button>
                )}
                <Button
                  onClick={onStop}
                  disabled={isStopping}
                  size="sm"
                  variant="destructive"
                >
                  {isStopping ? (
                    <Loader2 className="animate-spin h-4 w-4 mr-2" />
                  ) : (
                    <StopCircle className="h-4 w-4 mr-2" />
                  )}
                  {t('conversations.stop', 'Stop')}
                </Button>
              </>
            ) : (
              <Button
                onClick={handleSubmit}
                disabled={!localMessage.trim()}
                size="icon"
                className="h-11 w-11 flex-shrink-0"
              >
                <Send className="h-4 w-4" />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default MessageInput;
