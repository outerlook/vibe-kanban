import { useCallback, useState } from 'react';
import { Clipboard, Check } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useEntries } from '@/contexts/EntriesContext';
import { writeClipboardViaBridge } from '@/vscode/bridge';
import { conversationToMarkdown } from '@/utils/conversationToMarkdown';

export function CopyConversationButton() {
  const { entries } = useEntries();
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    const markdown = conversationToMarkdown(entries);
    if (!markdown) return;
    try {
      await writeClipboardViaBridge(markdown);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // bridge handles fallback
    }
  }, [entries]);

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={handleCopy}
            aria-label={copied ? 'Copied!' : 'Copy conversation as Markdown'}
          >
            {copied ? (
              <Check className="h-3.5 w-3.5 text-green-500" />
            ) : (
              <Clipboard className="h-3.5 w-3.5 text-muted-foreground" />
            )}
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {copied ? 'Copied!' : 'Copy conversation'}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
