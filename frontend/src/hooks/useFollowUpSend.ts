import { useCallback, useRef, useState } from 'react';
import { sessionsApi } from '@/lib/api';
import type { CreateFollowUpAttempt } from 'shared/types';

type Args = {
  sessionId?: string;
  message: string;
  conflictMarkdown: string | null;
  reviewMarkdown: string;
  clickedMarkdown?: string;
  selectedVariant: string | null;
  clearComments: () => void;
  clearClickedElements?: () => void;
  onAfterSendCleanup: () => void;
};

export function useFollowUpSend({
  sessionId,
  message,
  conflictMarkdown,
  reviewMarkdown,
  clickedMarkdown,
  selectedVariant,
  clearComments,
  clearClickedElements,
  onAfterSendCleanup,
}: Args) {
  const [isSendingFollowUp, setIsSendingFollowUp] = useState(false);
  const [followUpError, setFollowUpError] = useState<string | null>(null);
  // True when follow-up was queued due to global concurrency limit
  const [isGloballyQueued, setIsGloballyQueued] = useState(false);
  // Sync guard to prevent duplicate calls before React state updates
  const isSendingRef = useRef(false);

  const onSendFollowUp = useCallback(async () => {
    if (isSendingRef.current) return;
    isSendingRef.current = true;

    if (!sessionId) {
      isSendingRef.current = false;
      return;
    }
    const extraMessage = message.trim();
    const finalPrompt = [
      conflictMarkdown,
      clickedMarkdown?.trim(),
      reviewMarkdown?.trim(),
      extraMessage,
    ]
      .filter(Boolean)
      .join('\n\n');
    if (!finalPrompt) {
      isSendingRef.current = false;
      return;
    }
    try {
      setIsSendingFollowUp(true);
      setFollowUpError(null);
      setIsGloballyQueued(false);
      const body: CreateFollowUpAttempt = {
        prompt: finalPrompt,
        variant: selectedVariant,
        retry_process_id: null,
        force_when_dirty: null,
        perform_git_reset: null,
      };
      const result = await sessionsApi.followUp(sessionId, body);
      if (result.status === 'queued') {
        // Follow-up was queued due to global concurrency limit
        setIsGloballyQueued(true);
      }
      clearComments();
      clearClickedElements?.();
      onAfterSendCleanup();
      // Don't call jumpToLogsTab() - preserves focus on the follow-up editor
    } catch (error: unknown) {
      const err = error as { message?: string };
      setFollowUpError(
        `Failed to start follow-up execution: ${err.message ?? 'Unknown error'}`
      );
    } finally {
      isSendingRef.current = false;
      setIsSendingFollowUp(false);
    }
  }, [
    sessionId,
    message,
    conflictMarkdown,
    reviewMarkdown,
    clickedMarkdown,
    selectedVariant,
    clearComments,
    clearClickedElements,
    onAfterSendCleanup,
  ]);

  return {
    isSendingFollowUp,
    followUpError,
    setFollowUpError,
    onSendFollowUp,
    isGloballyQueued,
    setIsGloballyQueued,
  } as const;
}
