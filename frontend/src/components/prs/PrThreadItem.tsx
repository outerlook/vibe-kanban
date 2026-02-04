import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { MessageSquare, Code, ExternalLink, MessageSquarePlus } from 'lucide-react';
import type { UnifiedPrComment } from 'shared/types';
import { cn, formatDateTime } from '@/lib/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
} from '@/components/ui/dropdown-menu';
import { NewConversationDialog } from '@/components/dialogs/conversations/NewConversationDialog';
import { paths } from '@/lib/paths';

export interface PrThreadItemProps {
  thread: UnifiedPrComment;
  projectId: string;
  headBranch: string;
  className?: string;
}

/**
 * Renders a diff hunk with syntax highlighting for added/removed lines
 */
function DiffHunk({ diffHunk }: { diffHunk: string }) {
  const lines = diffHunk.split('\n');

  return (
    <pre className="mt-2 p-2 bg-secondary rounded text-xs font-mono overflow-x-auto max-h-32 overflow-y-auto">
      {lines.map((line, i) => {
        let lineClass = 'block';
        if (line.startsWith('+') && !line.startsWith('+++')) {
          lineClass =
            'block bg-green-500/20 text-green-700 dark:text-green-400';
        } else if (line.startsWith('-') && !line.startsWith('---')) {
          lineClass = 'block bg-red-500/20 text-red-700 dark:text-red-400';
        } else if (line.startsWith('@@')) {
          lineClass = 'block text-muted-foreground';
        }
        return (
          <code key={i} className={lineClass}>
            {line}
          </code>
        );
      })}
    </pre>
  );
}

/**
 * PrThreadItem - Renders a single PR comment thread item
 *
 * Handles both general conversation comments and inline code review comments
 * with appropriate styling for each type.
 */
export function PrThreadItem({
  thread,
  projectId,
  headBranch,
  className,
}: PrThreadItemProps) {
  const { t } = useTranslation(['prs']);
  const navigate = useNavigate();
  const isReview = thread.comment_type === 'review';
  const Icon = isReview ? Code : MessageSquare;

  const [contextMenu, setContextMenu] = useState({
    open: false,
    x: 0,
    y: 0,
  });

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ open: true, x: e.clientX, y: e.clientY });
  }, []);

  const closeContextMenu = useCallback(() => {
    setContextMenu({ open: false, x: 0, y: 0 });
  }, []);

  const handleNewConversation = useCallback(async () => {
    closeContextMenu();
    const result = await NewConversationDialog.show({
      projectId,
      defaultBaseBranch: headBranch,
    });

    if (result) {
      navigate(paths.conversation(projectId, result.id));
    }
  }, [projectId, headBranch, navigate, closeContextMenu]);

  return (
    <div
      className={cn(
        'p-3 bg-muted/50 rounded-md border border-border overflow-hidden',
        className
      )}
      onContextMenu={handleContextMenu}
    >
      {/* Header */}
      <div className="flex items-center justify-between gap-2 mb-2">
        <div className="flex items-center gap-2 min-w-0">
          <Icon className="w-4 h-4 text-muted-foreground flex-shrink-0" />
          <span className="font-medium text-sm">@{thread.author}</span>
          {isReview && (
            <span className="text-xs text-muted-foreground bg-secondary px-1.5 py-0.5 rounded">
              Review
            </span>
          )}
        </div>
        <div className="flex items-center gap-1 text-xs text-muted-foreground flex-shrink-0">
          <span>{formatDateTime(thread.created_at)}</span>
          {thread.url && (
            <a
              href={thread.url}
              target="_blank"
              rel="noopener noreferrer"
              className="hover:text-foreground transition-colors"
              aria-label="Open in GitHub"
              onClick={(e) => e.stopPropagation()}
            >
              <ExternalLink className="w-3 h-3" />
            </a>
          )}
        </div>
      </div>

      {/* File path for review comments */}
      {isReview && thread.path && (
        <div className="text-xs font-mono text-primary/70 mb-1">
          {thread.path}
          {thread.line ? `:${thread.line}` : ''}
        </div>
      )}

      {/* Diff hunk for review comments */}
      {isReview && thread.diff_hunk && <DiffHunk diffHunk={thread.diff_hunk} />}

      {/* Comment body */}
      <p className="text-sm text-muted-foreground whitespace-pre-wrap break-words mt-2">
        {thread.body}
      </p>

      {/* Context Menu */}
      <DropdownMenu
        open={contextMenu.open}
        onOpenChange={(open) => {
          if (!open) closeContextMenu();
        }}
      >
        <DropdownMenuContent
          style={{
            position: 'fixed',
            left: contextMenu.x,
            top: contextMenu.y,
          }}
          onCloseAutoFocus={(e) => e.preventDefault()}
        >
          <DropdownMenuItem onClick={handleNewConversation}>
            <MessageSquarePlus className="h-4 w-4" />
            {t('prs:newConversation', { defaultValue: 'New Conversation' })}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
