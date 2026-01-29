import { Virtuoso, VirtuosoHandle } from 'react-virtuoso';
import { useCallback, useEffect, useRef, useState } from 'react';

import DisplayConversationEntry from '../NormalizedConversation/DisplayConversationEntry';
import { useEntries } from '@/contexts/EntriesContext';
import {
  AddEntryType,
  HistoryMode,
  PatchTypeWithKey,
  useConversationHistory,
} from '@/hooks/useConversationHistory';
import { ArrowDown, Loader2 } from 'lucide-react';
import type { TaskWithAttemptStatus, WorkspaceWithSession } from 'shared/types';
import { ApprovalFormProvider } from '@/contexts/ApprovalFormContext';
import { ExecutionProcessesProvider } from '@/contexts/ExecutionProcessesContext';
import type { ExecutionProcessesSource } from '@/hooks/useExecutionProcesses';

type VirtualizedListMode =
  | { type: 'workspace'; attempt: WorkspaceWithSession; task?: TaskWithAttemptStatus }
  | { type: 'conversation'; conversationSessionId: string };

interface VirtualizedListProps {
  mode: VirtualizedListMode;
}

const AUTO_SCROLL_THRESHOLD = 100;
const BUTTON_VISIBILITY_THRESHOLD = 500;
const FIRST_ITEM_INDEX = 100000;

const VirtualizedListInner = ({ mode }: VirtualizedListProps) => {
  const [entries, setEntriesState] = useState<PatchTypeWithKey[]>([]);
  const [firstItemIndex, setFirstItemIndex] = useState(FIRST_ITEM_INDEX);
  const [loading, setLoading] = useState(true);
  const [atBottom, setAtBottom] = useState(true);
  const [unseenMessages, setUnseenMessages] = useState(0);
  const [bottomOffset, setBottomOffset] = useState(0);
  const didInitScrollRef = useRef(false);
  const previousEntryCountRef = useRef(0);
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const { setEntries, reset } = useEntries();

  const modeId =
    mode.type === 'workspace' ? mode.attempt.id : mode.conversationSessionId;

  useEffect(() => {
    setLoading(true);
    setEntriesState([]);
    setFirstItemIndex(FIRST_ITEM_INDEX);
    previousEntryCountRef.current = 0;
    didInitScrollRef.current = false;
    setAtBottom(true);
    setUnseenMessages(0);
    setBottomOffset(0);
    reset();
  }, [modeId, reset]);

  useEffect(() => {
    if (atBottom) {
      setUnseenMessages(0);
    }
  }, [atBottom]);

  const scrollToBottom = useCallback((behavior: 'auto' | 'smooth' = 'auto') => {
    virtuosoRef.current?.scrollToIndex({
      index: 'LAST',
      align: 'end',
      behavior,
    });
  }, []);

  const onEntriesUpdated = useCallback(
    (newEntries: PatchTypeWithKey[], addType: AddEntryType, newLoading: boolean) => {
      const previousCount = previousEntryCountRef.current;
      const nextCount = newEntries.length;
      const addedCount = nextCount - previousCount;
      previousEntryCountRef.current = nextCount;
      const wasLoading = loading;

      if (addType === 'historic' && addedCount > 0) {
        setFirstItemIndex((prev) => prev - addedCount);
      }

      setEntriesState(newEntries);
      setEntries(newEntries);

      if (loading) {
        setLoading(newLoading);
      }

      if (wasLoading || addedCount <= 0) {
        return;
      }

      if (addType === 'running') {
        if (!atBottom) {
          setUnseenMessages((prev) => prev + addedCount);
        }
      }
    },
    [loading, atBottom, setEntries]
  );

  const historyMode: HistoryMode =
    mode.type === 'workspace'
      ? { type: 'workspace', attempt: mode.attempt }
      : { type: 'conversation', conversationSessionId: mode.conversationSessionId };

  const { loadMoreHistory, hasMoreHistory, isLoadingMore } = useConversationHistory({
    mode: historyMode,
    onEntriesUpdated,
  });

  const handleStartReached = useCallback(() => {
    if (!hasMoreHistory || isLoadingMore || loading) return;
    loadMoreHistory();
  }, [hasMoreHistory, isLoadingMore, loadMoreHistory, loading]);

  const handleAtBottomStateChange = useCallback((bottom: boolean) => {
    setAtBottom(bottom);
    if (bottom) {
      setUnseenMessages(0);
    }
  }, []);

  useEffect(() => {
    if (loading || didInitScrollRef.current || entries.length === 0) {
      return;
    }

    didInitScrollRef.current = true;
    requestAnimationFrame(() => {
      scrollToBottom();
    });
  }, [loading, entries.length, scrollToBottom]);

  const renderItem = useCallback(
    (_index: number, data: PatchTypeWithKey) => {
      if (data.type === 'STDOUT') {
        return <p>{data.content}</p>;
      }
      if (data.type === 'STDERR') {
        return <p>{data.content}</p>;
      }
      if (data.type === 'NORMALIZED_ENTRY' || data.type === 'ENTRY_GROUP') {
        return (
          <DisplayConversationEntry
            expansionKey={data.patchKey}
            entry={data.content}
            executionProcessId={data.executionProcessId}
            taskAttempt={mode.type === 'workspace' ? mode.attempt : undefined}
            task={mode.type === 'workspace' ? mode.task : undefined}
          />
        );
      }
      return null;
    },
    [mode]
  );

  const shouldShowButton = unseenMessages > 0 || bottomOffset > BUTTON_VISIBILITY_THRESHOLD;

  return (
    <ApprovalFormProvider>
      <div className="flex-1 relative">
        <Virtuoso
          ref={virtuosoRef}
          className="h-full"
          data={entries}
          firstItemIndex={firstItemIndex}
          initialTopMostItemIndex={entries.length > 0 ? entries.length - 1 : 0}
          computeItemKey={(_index, data) => `l-${data.patchKey}`}
          itemContent={renderItem}
          startReached={handleStartReached}
          atBottomStateChange={handleAtBottomStateChange}
          atBottomThreshold={AUTO_SCROLL_THRESHOLD}
          followOutput="auto"
          onScroll={(e) => {
            const target = e.target as HTMLElement;
            const offset = target.scrollHeight - target.scrollTop - target.clientHeight;
            setBottomOffset(offset);
          }}
          components={{
            Header: () =>
              isLoadingMore ? (
                <div className="flex items-center justify-center py-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                </div>
              ) : (
                <div className="h-2" />
              ),
            Footer: () => <div className="h-2" />,
          }}
        />
        {shouldShowButton && (
          <div className="absolute bottom-4 right-4 z-10">
            <button
              onClick={() => {
                scrollToBottom('smooth');
                setUnseenMessages(0);
              }}
              className="bg-primary text-primary-foreground px-3 py-2 rounded-full shadow-lg flex items-center gap-2 text-sm"
            >
              <ArrowDown className="h-4 w-4" />
              <span>
                {unseenMessages > 0
                  ? `${unseenMessages} new message${unseenMessages === 1 ? '' : 's'}`
                  : 'Jump to bottom'}
              </span>
            </button>
          </div>
        )}
      </div>
      {loading && (
        <div className="float-left top-0 left-0 w-full h-full bg-primary flex flex-col gap-2 justify-center items-center">
          <Loader2 className="h-8 w-8 animate-spin" />
          <p>Loading History</p>
        </div>
      )}
    </ApprovalFormProvider>
  );
};

const VirtualizedList = ({ mode }: VirtualizedListProps) => {
  // Convert mode to ExecutionProcessesSource
  const source: ExecutionProcessesSource =
    mode.type === 'workspace'
      ? { type: 'workspace', workspaceId: mode.attempt.id }
      : { type: 'conversation', conversationSessionId: mode.conversationSessionId };

  return (
    <ExecutionProcessesProvider source={source}>
      <VirtualizedListInner mode={mode} />
    </ExecutionProcessesProvider>
  );
};

export default VirtualizedList;
export type { VirtualizedListMode };
