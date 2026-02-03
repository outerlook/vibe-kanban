import { MessageSquare, Code, ExternalLink } from 'lucide-react';
import type { UnifiedPrComment } from 'shared/types';
import { cn } from '@/lib/utils';

export interface PrThreadItemProps {
  thread: UnifiedPrComment;
  className?: string;
}

function formatDate(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleString();
  } catch {
    return dateStr;
  }
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
export function PrThreadItem({ thread, className }: PrThreadItemProps) {
  const isReview = thread.comment_type === 'review';
  const Icon = isReview ? Code : MessageSquare;

  return (
    <div
      className={cn(
        'p-3 bg-muted/50 rounded-md border border-border overflow-hidden',
        className
      )}
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
          <span>{formatDate(thread.created_at)}</span>
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
    </div>
  );
}
