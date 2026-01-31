import { ChevronDown, ChevronRight, Layers } from 'lucide-react';
import { useExpandable } from '@/stores/useExpandableStore';
import { cn } from '@/lib/utils';
import type {
  GroupSummary,
  NormalizedEntry,
  TaskWithAttemptStatus,
  WorkspaceWithSession,
} from 'shared/types';
import DisplayConversationEntry from './DisplayConversationEntry';

interface CollapsibleEntryGroupProps {
  entries: NormalizedEntry[];
  summary: GroupSummary;
  expansionKey: string;
  executionProcessId: string;
  taskAttempt?: WorkspaceWithSession;
  task?: TaskWithAttemptStatus;
  conversationId?: string;
}

/**
 * Formats a GroupSummary into a human-readable string.
 * Produces strings like: "2 commands • 1 file read • 3 searches"
 * Only includes categories with count > 0 and uses singular/plural forms.
 */
export function formatSummaryText(summary: GroupSummary): string {
  const parts: string[] = [];

  if (summary.commands > 0) {
    parts.push(`${summary.commands} ${summary.commands === 1 ? 'command' : 'commands'}`);
  }
  if (summary.file_reads > 0) {
    parts.push(`${summary.file_reads} ${summary.file_reads === 1 ? 'file read' : 'file reads'}`);
  }
  if (summary.file_edits > 0) {
    parts.push(`${summary.file_edits} ${summary.file_edits === 1 ? 'file edit' : 'file edits'}`);
  }
  if (summary.searches > 0) {
    parts.push(`${summary.searches} ${summary.searches === 1 ? 'search' : 'searches'}`);
  }
  if (summary.web_fetches > 0) {
    parts.push(`${summary.web_fetches} ${summary.web_fetches === 1 ? 'web fetch' : 'web fetches'}`);
  }
  if (summary.tools > 0) {
    parts.push(`${summary.tools} ${summary.tools === 1 ? 'tool' : 'tools'}`);
  }
  if (summary.system_messages > 0) {
    parts.push(`${summary.system_messages} ${summary.system_messages === 1 ? 'system message' : 'system messages'}`);
  }
  if (summary.errors > 0) {
    parts.push(`${summary.errors} ${summary.errors === 1 ? 'error' : 'errors'}`);
  }
  if (summary.thinking > 0) {
    parts.push(`${summary.thinking} thinking`);
  }

  return parts.length > 0 ? parts.join(' • ') : 'Empty group';
}

export function CollapsibleEntryGroup({
  entries,
  summary,
  expansionKey,
  executionProcessId,
  taskAttempt,
  task,
  conversationId,
}: CollapsibleEntryGroupProps) {
  const [expanded, toggle] = useExpandable(
    `entry-group:${expansionKey}`,
    false
  );

  const summaryText = formatSummaryText(summary);

  return (
    <div className="w-full">
      <button
        onClick={(e: React.MouseEvent) => {
          e.preventDefault();
          toggle();
        }}
        className={cn(
          'w-full px-3 py-2 flex items-center gap-2 text-left',
          'bg-muted/30 hover:bg-muted/50 transition-colors',
          'border border-border/50 rounded-sm',
          'cursor-pointer'
        )}
        aria-expanded={expanded}
      >
        <span className="flex-shrink-0 text-muted-foreground">
          {expanded ? (
            <ChevronDown className="h-4 w-4" />
          ) : (
            <ChevronRight className="h-4 w-4" />
          )}
        </span>
        <Layers className="h-3.5 w-3.5 text-muted-foreground flex-shrink-0" />
        <span className="text-sm text-muted-foreground truncate">
          {summaryText}
        </span>
        <span className="ml-auto text-xs text-muted-foreground/70 flex-shrink-0">
          {entries.length} {entries.length === 1 ? 'entry' : 'entries'}
        </span>
      </button>

      {expanded && (
        <div className="border-l-2 border-border/50 ml-2 pl-3 mt-1 space-y-1">
          {entries.map((entry, index) => (
            <DisplayConversationEntry
              key={`${expansionKey}-entry-${index}`}
              entry={entry}
              expansionKey={`${expansionKey}-entry-${index}`}
              executionProcessId={executionProcessId}
              taskAttempt={taskAttempt}
              task={task}
              conversationId={conversationId}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export default CollapsibleEntryGroup;
