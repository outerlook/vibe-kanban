import { useState, useCallback, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Send, StopCircle, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { cn } from '@/lib/utils';

interface MessageInputProps {
  onSend: (content: string) => void;
  disabled?: boolean;
  placeholder?: string;
  onStop?: () => void;
  isStopping?: boolean;
  showStopButton?: boolean;
}

export function MessageInput({
  onSend,
  disabled = false,
  placeholder,
  onStop,
  isStopping = false,
  showStopButton = false,
}: MessageInputProps) {
  const { t } = useTranslation('common');
  const [content, setContent] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const defaultPlaceholder = t('conversations.messagePlaceholder', {
    defaultValue: 'Type a message...',
  });

  const handleSubmit = useCallback(() => {
    const trimmed = content.trim();
    if (!trimmed || disabled) return;

    onSend(trimmed);
    setContent('');

    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  }, [content, disabled, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit]
  );

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
  }, [content]);

  return (
    <div className="border-t bg-background p-4">
      <div className="max-w-3xl mx-auto">
        <div className="flex items-end gap-2">
          <Textarea
            ref={textareaRef}
            value={content}
            onChange={(e) => setContent(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder ?? defaultPlaceholder}
            disabled={disabled}
            className={cn(
              'min-h-[44px] max-h-[200px] resize-none',
              disabled && 'opacity-50'
            )}
            rows={1}
          />
          {showStopButton ? (
            <Button
              onClick={onStop}
              disabled={isStopping}
              size="sm"
              variant="destructive"
              className="flex-shrink-0"
            >
              {isStopping ? (
                <Loader2 className="animate-spin h-4 w-4 mr-2" />
              ) : (
                <StopCircle className="h-4 w-4 mr-2" />
              )}
              {t('conversations.stop', { defaultValue: 'Stop' })}
            </Button>
          ) : (
            <Button
              onClick={handleSubmit}
              disabled={disabled || !content.trim()}
              size="icon"
              className="h-11 w-11 flex-shrink-0"
            >
              <Send className="h-4 w-4" />
            </Button>
          )}
        </div>
        {disabled && !showStopButton && (
          <p className="text-xs text-muted-foreground mt-2">
            {t('conversations.waitingForResponse', {
              defaultValue: 'Waiting for response...',
            })}
          </p>
        )}
      </div>
    </div>
  );
}

export default MessageInput;
