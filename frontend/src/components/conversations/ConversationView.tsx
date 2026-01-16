import { useEffect, useRef, useCallback, useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Bot, User } from 'lucide-react';
import { Loader } from '@/components/ui/loader';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import {
  useConversation,
  useConversationExecutions,
} from '@/hooks/useConversations';
import { useJsonPatchWsStream } from '@/hooks/useJsonPatchWsStream';
import type {
  ConversationMessage,
  ExecutionProcess,
  ExecutionProcessStatus,
  PatchType,
  NormalizedEntry,
} from 'shared/types';
import { cn } from '@/lib/utils';

interface ConversationViewProps {
  conversationId: string;
}

interface MessageBubbleProps {
  role: 'user' | 'assistant';
  children: React.ReactNode;
}

function MessageBubble({ role, children }: MessageBubbleProps) {
  const isUser = role === 'user';

  return (
    <div className={cn('flex gap-3 py-3', isUser ? 'justify-end' : '')}>
      {!isUser && (
        <div className="flex-shrink-0 w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center">
          <Bot className="h-4 w-4 text-primary" />
        </div>
      )}
      <div
        className={cn(
          'max-w-[80%] rounded-lg px-4 py-2',
          isUser
            ? 'bg-primary text-primary-foreground'
            : 'bg-muted text-foreground'
        )}
      >
        {children}
      </div>
      {isUser && (
        <div className="flex-shrink-0 w-8 h-8 rounded-full bg-accent flex items-center justify-center">
          <User className="h-4 w-4" />
        </div>
      )}
    </div>
  );
}

interface StreamingResponseProps {
  executionProcess: ExecutionProcess;
}

function StreamingResponse({ executionProcess }: StreamingResponseProps) {
  const isRunning = executionProcess.status === ('running' as ExecutionProcessStatus);
  const endpoint = `/api/execution-processes/${executionProcess.id}/normalized-logs/ws`;

  const initialData = useCallback(() => [] as PatchType[], []);

  const { data: patches } = useJsonPatchWsStream<PatchType[]>(
    endpoint,
    isRunning,
    initialData
  );

  const content = useMemo(() => {
    if (!patches || patches.length === 0) return null;

    // Extract text content from normalized entries
    const textParts: string[] = [];
    for (const patch of patches) {
      if (patch.type === 'NORMALIZED_ENTRY') {
        const entry = patch.content as NormalizedEntry;
        if (
          entry.entry_type.type === 'assistant_message' ||
          entry.entry_type.type === 'thinking'
        ) {
          if (entry.content) {
            textParts.push(entry.content);
          }
        }
      }
    }
    return textParts.join('\n');
  }, [patches]);

  if (!content && isRunning) {
    return (
      <div className="flex items-center gap-2 text-muted-foreground">
        <Loader size={16} />
        <span className="text-sm">Thinking...</span>
      </div>
    );
  }

  if (!content) {
    return null;
  }

  return (
    <div className="prose prose-sm dark:prose-invert max-w-none">
      <WYSIWYGEditor value={content} disabled />
      {isRunning && (
        <div className="flex items-center gap-2 mt-2 text-muted-foreground">
          <Loader size={14} />
        </div>
      )}
    </div>
  );
}

export function ConversationView({ conversationId }: ConversationViewProps) {
  const { t } = useTranslation('common');
  const scrollRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  const { data: conversation, isLoading, error } = useConversation(conversationId);
  const { data: executions } = useConversationExecutions(conversationId);

  // Find active execution for streaming
  const activeExecution = useMemo(() => {
    if (!executions) return null;
    return executions.find(
      (ep) => ep.status === ('running' as ExecutionProcessStatus)
    );
  }, [executions]);


  // Auto-scroll on new messages
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [conversation?.messages, autoScroll]);

  // Handle scroll to detect user scrolling up
  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;
    setAutoScroll(isAtBottom);
  }, []);

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
    <div
      ref={scrollRef}
      className="flex-1 overflow-y-auto px-4"
      onScroll={handleScroll}
    >
      <div className="max-w-3xl mx-auto py-4 space-y-2">
        {conversation.messages.map((message: ConversationMessage) => (
          <MessageBubble key={message.id} role={message.role}>
            {message.role === 'user' ? (
              <div className="whitespace-pre-wrap">{message.content}</div>
            ) : (
              <div className="prose prose-sm dark:prose-invert max-w-none">
                <WYSIWYGEditor value={message.content} disabled />
              </div>
            )}
          </MessageBubble>
        ))}

        {/* Show streaming response for active execution */}
        {activeExecution && (
          <MessageBubble role="assistant">
            <StreamingResponse executionProcess={activeExecution} />
          </MessageBubble>
        )}
      </div>
    </div>
  );
}

export default ConversationView;
