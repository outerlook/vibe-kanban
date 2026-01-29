import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Send, StopCircle, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { imagesApi } from '@/lib/api';
import { cn } from '@/lib/utils';

interface MessageInputProps {
  onSend: (content: string) => void;
  conversationId: string;
  disabled?: boolean;
  placeholder?: string;
  onStop?: () => void;
  isStopping?: boolean;
  showStopButton?: boolean;
}

export function MessageInput({
  onSend,
  conversationId,
  disabled = false,
  placeholder,
  onStop,
  isStopping = false,
  showStopButton = false,
}: MessageInputProps) {
  const { t } = useTranslation('common');
  const [content, setContent] = useState('');

  const defaultPlaceholder = t('conversations.messagePlaceholder', {
    defaultValue: 'Type a message...',
  });

  const handleSubmit = useCallback(() => {
    const trimmed = content.trim();
    if (!trimmed || disabled) return;

    onSend(trimmed);
    setContent('');
  }, [content, disabled, onSend]);

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

          setContent((prev) =>
            prev ? `${prev}\n\n${imageMarkdown}` : imageMarkdown
          );
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
        <div className="flex items-end gap-2">
          <div
            className={cn(
              'flex-1 border rounded-md px-3 py-2 bg-background',
              disabled && 'opacity-50'
            )}
          >
            <WYSIWYGEditor
              placeholder={placeholder ?? defaultPlaceholder}
              value={content}
              onChange={setContent}
              disabled={disabled}
              onPasteFiles={handlePasteFiles}
              onCmdEnter={handleSubmit}
              className="min-h-[28px]"
            />
          </div>
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
