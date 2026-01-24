import {
  DataWithScrollModifier,
  VirtuosoMessageList,
  VirtuosoMessageListLicense,
  VirtuosoMessageListMethods,
  VirtuosoMessageListProps,
  useVirtuosoLocation,
  useVirtuosoMethods,
} from '@virtuoso.dev/message-list';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

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

interface MessageListContext {
  mode: VirtualizedListMode;
}

const INITIAL_TOP_ITEM = { index: 'LAST' as const, align: 'end' as const };

const TOP_LOAD_THRESHOLD = 8;
const AUTO_SCROLL_THRESHOLD = 100;
const BUTTON_VISIBILITY_THRESHOLD = 500;

const ItemContent: VirtuosoMessageListProps<
  PatchTypeWithKey,
  MessageListContext
>['ItemContent'] = ({ data, context }) => {
  const mode = context?.mode;

  if (data.type === 'STDOUT') {
    return <p>{data.content}</p>;
  }
  if (data.type === 'STDERR') {
    return <p>{data.content}</p>;
  }
  if (data.type === 'NORMALIZED_ENTRY') {
    return (
      <DisplayConversationEntry
        expansionKey={data.patchKey}
        entry={data.content}
        executionProcessId={data.executionProcessId}
        taskAttempt={mode?.type === 'workspace' ? mode.attempt : undefined}
        task={mode?.type === 'workspace' ? mode.task : undefined}
      />
    );
  }
  if (data.type === 'ENTRY_GROUP') {
    return (
      <DisplayConversationEntry
        expansionKey={data.patchKey}
        entry={data.content}
        executionProcessId={data.executionProcessId}
        taskAttempt={mode?.type === 'workspace' ? mode.attempt : undefined}
        task={mode?.type === 'workspace' ? mode.task : undefined}
      />
    );
  }

  return null;
};

const computeItemKey: VirtuosoMessageListProps<
  PatchTypeWithKey,
  MessageListContext
>['computeItemKey'] = ({ data }) => `l-${data.patchKey}`;

const VirtualizedListInner = ({ mode }: VirtualizedListProps) => {
  const [channelData, setChannelData] =
    useState<DataWithScrollModifier<PatchTypeWithKey> | null>(null);
  const [loading, setLoading] = useState(true);
  const [atBottom, setAtBottom] = useState(true);
  const [unseenMessages, setUnseenMessages] = useState(0);
  const didInitScrollRef = useRef(false);
  const previousEntryCountRef = useRef(0);
  const bottomOffsetRef = useRef(0);
  const messageListRef = useRef<VirtuosoMessageListMethods | null>(null);
  const { setEntries, reset } = useEntries();

  // Derive modeId for dependency tracking
  const modeId =
    mode.type === 'workspace' ? mode.attempt.id : mode.conversationSessionId;

  useEffect(() => {
    setLoading(true);
    setChannelData(null);
    previousEntryCountRef.current = 0;
    bottomOffsetRef.current = 0;
    didInitScrollRef.current = false;
    setAtBottom(true);
    setUnseenMessages(0);
    reset();
  }, [modeId, reset]);

  useEffect(() => {
    if (atBottom) {
      setUnseenMessages(0);
    }
  }, [atBottom]);

  const scrollToBottom = useCallback((behavior: ScrollBehavior = 'auto') => {
    messageListRef.current?.scrollToItem({
      index: 'LAST',
      align: 'end',
      behavior,
    });
  }, []);

  const onEntriesUpdated = (
    newEntries: PatchTypeWithKey[],
    addType: AddEntryType,
    newLoading: boolean
  ) => {
    const previousCount = previousEntryCountRef.current;
    const nextCount = newEntries.length;
    const addedCount = nextCount - previousCount;
    previousEntryCountRef.current = nextCount;
    const wasLoading = loading;

    setChannelData({ data: newEntries });
    setEntries(newEntries);

    if (loading) {
      setLoading(newLoading);
    }

    if (wasLoading || addedCount <= 0) {
      return;
    }

    if (addType === 'running') {
      const isNearBottom = bottomOffsetRef.current < AUTO_SCROLL_THRESHOLD;
      if (atBottom || isNearBottom) {
        requestAnimationFrame(() => {
          scrollToBottom('smooth');
        });
      } else {
        setUnseenMessages((prev) => prev + addedCount);
      }
      return;
    }

    if (addType === 'historic') {
      requestAnimationFrame(() => {
        messageListRef.current?.scrollToItem({
          index: addedCount,
          align: 'start',
        });
      });
    }
  };

  // Convert VirtualizedListMode to HistoryMode
  const historyMode: HistoryMode =
    mode.type === 'workspace'
      ? { type: 'workspace', attempt: mode.attempt }
      : { type: 'conversation', conversationSessionId: mode.conversationSessionId };

  const { loadMoreHistory, hasMoreHistory, isLoadingMore } =
    useConversationHistory({ mode: historyMode, onEntriesUpdated });

  const handleScroll = useCallback(
    ({
      listOffset,
      bottomOffset,
    }: {
      listOffset: number;
      bottomOffset?: number;
    }) => {
      if (bottomOffset !== undefined) {
        bottomOffsetRef.current = bottomOffset;
        const isAtBottom = bottomOffset < AUTO_SCROLL_THRESHOLD;
        setAtBottom(isAtBottom);
        if (isAtBottom) {
          setUnseenMessages(0);
        }
      }
      if (!hasMoreHistory || isLoadingMore || loading) return;
      if (listOffset >= -TOP_LOAD_THRESHOLD) {
        loadMoreHistory();
      }
    },
    [hasMoreHistory, isLoadingMore, loadMoreHistory, loading]
  );

  const messageListContext = useMemo(() => ({ mode }), [mode]);

  const StickyFooter: VirtuosoMessageListProps<
    PatchTypeWithKey,
    MessageListContext
  >['StickyFooter'] = () => {
    const location = useVirtuosoLocation();
    const methods = useVirtuosoMethods();

    const shouldShowButton =
      unseenMessages > 0 || location.bottomOffset > BUTTON_VISIBILITY_THRESHOLD;

    if (!shouldShowButton) {
      return null;
    }

    const label =
      unseenMessages > 0
        ? `${unseenMessages} new message${unseenMessages === 1 ? '' : 's'}`
        : 'Jump to bottom';

    return (
      <div className="absolute bottom-4 right-4 z-10">
        <button
          onClick={() => {
            methods.scrollToItem({
              index: 'LAST',
              align: 'end',
              behavior: 'smooth',
            });
            setUnseenMessages(0);
          }}
          className="bg-primary text-primary-foreground px-3 py-2 rounded-full shadow-lg flex items-center gap-2 text-sm"
        >
          <ArrowDown className="h-4 w-4" />
          <span>{label}</span>
        </button>
      </div>
    );
  };

  useEffect(() => {
    const dataLength = channelData?.data?.length ?? 0;
    if (loading || didInitScrollRef.current || dataLength === 0) {
      return;
    }

    didInitScrollRef.current = true;
    requestAnimationFrame(() => {
      scrollToBottom();
    });
  }, [loading, channelData, scrollToBottom]);

  return (
    <ApprovalFormProvider>
      <VirtuosoMessageListLicense
        licenseKey={import.meta.env.VITE_PUBLIC_REACT_VIRTUOSO_LICENSE_KEY}
      >
        <VirtuosoMessageList<PatchTypeWithKey, MessageListContext>
          ref={messageListRef}
          className="flex-1 relative"
          data={channelData}
          initialLocation={INITIAL_TOP_ITEM}
          context={messageListContext}
          computeItemKey={computeItemKey}
          ItemContent={ItemContent}
          Header={() =>
            isLoadingMore ? (
              <div className="flex items-center justify-center py-2">
                <Loader2 className="h-4 w-4 animate-spin" />
              </div>
            ) : (
              <div className="h-2"></div>
            )
          }
          Footer={() => <div className="h-2"></div>}
          onScroll={handleScroll}
          StickyFooter={StickyFooter}
        />
      </VirtuosoMessageListLicense>
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
