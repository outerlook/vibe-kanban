// useConversationHistory.ts
import {
  CommandExitStatus,
  EntryGroup,
  ExecutionProcess,
  ExecutionProcessStatus,
  ExecutorAction,
  GroupSummary,
  NormalizedEntry,
  PatchType,
  ToolStatus,
  Workspace,
} from 'shared/types';
import { shouldShowInLogs } from '@/constants/processes';
import { useExecutionProcessesContext } from '@/contexts/ExecutionProcessesContext';
import { executionProcessesApi } from '@/lib/api';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { streamJsonPatchEntries } from '@/utils/streamJsonPatchEntries';

export type PatchTypeWithKey = PatchType & {
  patchKey: string;
  executionProcessId: string;
};

export type AddEntryType = 'initial' | 'running' | 'historic';

export type OnEntriesUpdated = (
  newEntries: PatchTypeWithKey[],
  addType: AddEntryType,
  loading: boolean
) => void;

type ExecutionProcessStaticInfo = {
  id: string;
  created_at: string;
  updated_at: string;
  executor_action: ExecutorAction;
};

type ExecutionProcessState = {
  executionProcess: ExecutionProcessStaticInfo;
  entries: PatchTypeWithKey[];
  hasMore: boolean;
  nextBeforeIndex: number | null;
};

type ExecutionProcessStateStore = Record<string, ExecutionProcessState>;

type HistoricEntriesPage = {
  entries: PatchTypeWithKey[];
  hasMore: boolean;
  nextBeforeIndex: number | null;
};

export type HistoryMode =
  | { type: 'workspace'; attempt: Workspace }
  | { type: 'conversation'; conversationSessionId: string };

interface UseConversationHistoryParams {
  mode: HistoryMode;
  onEntriesUpdated: OnEntriesUpdated;
}

interface UseConversationHistoryResult {
  loadMoreHistory: () => void;
  hasMoreHistory: boolean;
  isLoadingMore: boolean;
}

const MIN_INITIAL_ENTRIES = 10;
const NORMALIZED_ENTRIES_PAGE_SIZE = 80;

const makeLoadingPatch = (executionProcessId: string): PatchTypeWithKey => ({
  type: 'NORMALIZED_ENTRY',
  content: {
    entry_type: {
      type: 'loading',
    },
    content: '',
    timestamp: null,
    metadata: null,
  },
  patchKey: `${executionProcessId}:loading`,
  executionProcessId,
});

const nextActionPatch: (
  failed: boolean,
  execution_processes: number,
  needs_setup: boolean,
  setup_help_text?: string
) => PatchTypeWithKey = (
  failed,
  execution_processes,
  needs_setup,
  setup_help_text
) => ({
  type: 'NORMALIZED_ENTRY',
  content: {
    entry_type: {
      type: 'next_action',
      failed: failed,
      execution_processes: execution_processes,
      needs_setup: needs_setup,
      setup_help_text: setup_help_text ?? null,
    },
    content: '',
    timestamp: null,
    metadata: null,
  },
  patchKey: 'next_action',
  executionProcessId: '',
});

export const useConversationHistory = ({
  mode,
  onEntriesUpdated,
}: UseConversationHistoryParams): UseConversationHistoryResult => {
  // For conversation mode, we don't have execution processes context
  // For workspace mode, we use the existing execution processes context
  const {
    executionProcessesVisible: executionProcessesRaw,
    isLoading: isExecutionProcessesLoading,
    isConnected: isExecutionProcessesConnected,
  } = useExecutionProcessesContext();

  // Derive mode-specific values
  const modeId = mode.type === 'workspace' ? mode.attempt.id : mode.conversationSessionId;
  const executionProcesses = useRef<ExecutionProcess[]>(executionProcessesRaw);
  const displayedExecutionProcesses = useRef<ExecutionProcessStateStore>({});
  const loadedInitialEntries = useRef(false);
  const streamingProcessIdsRef = useRef<Set<string>>(new Set());
  const onEntriesUpdatedRef = useRef<OnEntriesUpdated | null>(null);
  const [hasMoreHistory, setHasMoreHistory] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const loadingMoreRef = useRef(false);

  const mergeIntoDisplayed = (
    mutator: (state: ExecutionProcessStateStore) => void
  ) => {
    const state = displayedExecutionProcesses.current;
    mutator(state);
  };

  const updateHasMoreHistory = useCallback(() => {
    const processes = executionProcesses.current ?? [];
    if (processes.length === 0) {
      setHasMoreHistory(false);
      return;
    }

    const sorted = [...processes].sort(
      (a, b) =>
        new Date(a.created_at as unknown as string).getTime() -
        new Date(b.created_at as unknown as string).getTime()
    );
    const loadedIds = new Set(
      Object.keys(displayedExecutionProcesses.current)
    );
    const earliestLoadedIndex = sorted.findIndex((p) => loadedIds.has(p.id));

    if (earliestLoadedIndex === -1) {
      setHasMoreHistory(false);
      return;
    }

    const earliestState =
      displayedExecutionProcesses.current[sorted[earliestLoadedIndex].id];
    const hasMoreOnEarliest =
      Boolean(earliestState?.hasMore) &&
      earliestState?.nextBeforeIndex !== null;
    const hasOlderProcess = earliestLoadedIndex > 0;

    setHasMoreHistory(hasMoreOnEarliest || hasOlderProcess);
  }, []);
  useEffect(() => {
    onEntriesUpdatedRef.current = onEntriesUpdated;
  }, [onEntriesUpdated]);

  // Keep executionProcesses up to date
  useEffect(() => {
    executionProcesses.current = executionProcessesRaw.filter((ep) =>
      shouldShowInLogs(ep.run_reason)
    );
  }, [executionProcessesRaw]);

  const loadNormalizedEntriesPage = useCallback(
    async (
      executionProcess: ExecutionProcess,
      beforeIndex?: number
    ): Promise<HistoricEntriesPage> => {
      try {
        const page = await executionProcessesApi.getNormalizedEntries(
          executionProcess.id,
          {
            beforeIndex,
            limit: NORMALIZED_ENTRIES_PAGE_SIZE,
          }
        );

        const entries: PatchTypeWithKey[] = page.entries.map((entry) => ({
          type: 'NORMALIZED_ENTRY' as const,
          content: entry.entry,
          patchKey: `${executionProcess.id}:${entry.entry_index}`,
          executionProcessId: executionProcess.id,
        }));

        return {
          entries,
          hasMore: page.has_more,
          nextBeforeIndex: page.next_before_index,
        };
      } catch (err) {
        console.warn(
          `Error loading normalized entries for execution process ${executionProcess.id}`,
          err
        );
        return {
          entries: [],
          hasMore: false,
          nextBeforeIndex: null,
        };
      }
    },
    []
  );

  const loadHistoricEntriesPage = useCallback(
    async (
      executionProcess: ExecutionProcess,
      beforeIndex?: number
    ): Promise<HistoricEntriesPage> => {
      if (executionProcess.executor_action.typ.type === 'ScriptRequest') {
        const url = `/api/execution-processes/${executionProcess.id}/raw-logs/ws`;
        const entries = await new Promise<PatchType[]>((resolve) => {
          const controller = streamJsonPatchEntries<PatchType>(url, {
            onFinished: (allEntries) => {
              controller.close();
              resolve(allEntries);
            },
            onError: (err) => {
              console.warn(
                `Error loading entries for historic execution process ${executionProcess.id}`,
                err
              );
              controller.close();
              resolve([]);
            },
          });
        });

        return {
          entries: entries.map((entry, index) => ({
            ...entry,
            patchKey: `${executionProcess.id}:${index}`,
            executionProcessId: executionProcess.id,
          })),
          hasMore: false,
          nextBeforeIndex: null,
        };
      }

      return loadNormalizedEntriesPage(executionProcess, beforeIndex);
    },
    [loadNormalizedEntriesPage]
  );

  const getLiveExecutionProcess = (
    executionProcessId: string
  ): ExecutionProcess | undefined => {
    return executionProcesses?.current.find(
      (executionProcess) => executionProcess.id === executionProcessId
    );
  };

  const patchWithKey = (
    patch: PatchType,
    executionProcessId: string,
    index: number | 'user'
  ) => {
    return {
      ...patch,
      patchKey: `${executionProcessId}:${index}`,
      executionProcessId,
    };
  };

  const flattenEntries = (
    executionProcessState: ExecutionProcessStateStore
  ): PatchTypeWithKey[] => {
    return Object.values(executionProcessState)
      .filter(
        (p) =>
          p.executionProcess.executor_action.typ.type ===
            'CodingAgentFollowUpRequest' ||
          p.executionProcess.executor_action.typ.type ===
            'CodingAgentInitialRequest'
      )
      .sort(
        (a, b) =>
          new Date(
            a.executionProcess.created_at as unknown as string
          ).getTime() -
          new Date(b.executionProcess.created_at as unknown as string).getTime()
      )
      .flatMap((p) => p.entries);
  };

  const categorizeEntry = (
    entry: PatchTypeWithKey
  ): 'message' | 'groupable' => {
    if (entry.type !== 'NORMALIZED_ENTRY') {
      return 'groupable';
    }
    const entryType = entry.content.entry_type.type;
    if (entryType === 'user_message' || entryType === 'assistant_message') {
      return 'message';
    }
    return 'groupable';
  };

  const computeGroupSummary = (entries: PatchTypeWithKey[]): GroupSummary => {
    const summary: GroupSummary = {
      commands: 0,
      file_reads: 0,
      file_edits: 0,
      searches: 0,
      web_fetches: 0,
      tools: 0,
      system_messages: 0,
      errors: 0,
      thinking: 0,
      token_usage: 0,
    };

    for (const entry of entries) {
      if (entry.type !== 'NORMALIZED_ENTRY') continue;

      const entryType = entry.content.entry_type;
      switch (entryType.type) {
        case 'tool_use': {
          const action = entryType.action_type.action;
          switch (action) {
            case 'command_run':
              summary.commands++;
              break;
            case 'file_read':
              summary.file_reads++;
              break;
            case 'file_edit':
              summary.file_edits++;
              break;
            case 'search':
              summary.searches++;
              break;
            case 'web_fetch':
              summary.web_fetches++;
              break;
            default:
              summary.tools++;
              break;
          }
          break;
        }
        case 'system_message':
          summary.system_messages++;
          break;
        case 'error_message':
          summary.errors++;
          break;
        case 'thinking':
          summary.thinking++;
          break;
        case 'token_usage':
          summary.token_usage++;
          break;
      }
    }

    return summary;
  };

  const groupConsecutiveNonMessages = (
    entries: PatchTypeWithKey[]
  ): PatchTypeWithKey[] => {
    const result: PatchTypeWithKey[] = [];
    let groupableAccumulator: PatchTypeWithKey[] = [];

    const flushAccumulator = () => {
      if (groupableAccumulator.length === 0) return;

      if (groupableAccumulator.length === 1) {
        result.push(groupableAccumulator[0]);
      } else {
        const firstEntry = groupableAccumulator[0];
        const lastEntry = groupableAccumulator[groupableAccumulator.length - 1];
        const groupContent: EntryGroup = {
          entries: groupableAccumulator.map((e) =>
            e.type === 'NORMALIZED_ENTRY' ? e.content : ({} as NormalizedEntry)
          ),
          summary: computeGroupSummary(groupableAccumulator),
        };
        const groupPatch: PatchTypeWithKey = {
          type: 'ENTRY_GROUP',
          content: groupContent,
          patchKey: `group:${firstEntry.patchKey}:${lastEntry.patchKey}`,
          executionProcessId: firstEntry.executionProcessId,
        };
        result.push(groupPatch);
      }
      groupableAccumulator = [];
    };

    for (const entry of entries) {
      const category = categorizeEntry(entry);
      if (category === 'message') {
        flushAccumulator();
        result.push(entry);
      } else {
        groupableAccumulator.push(entry);
      }
    }

    flushAccumulator();
    return result;
  };

  const getActiveAgentProcesses = (): ExecutionProcess[] => {
    return (
      executionProcesses?.current.filter(
        (p) =>
          p.status === ExecutionProcessStatus.running &&
          p.run_reason !== 'devserver'
      ) ?? []
    );
  };

  const flattenEntriesForEmit = useCallback(
    (executionProcessState: ExecutionProcessStateStore): PatchTypeWithKey[] => {
      // Flags to control Next Action bar emit
      let hasPendingApproval = false;
      let hasRunningProcess = false;
      let lastProcessFailedOrKilled = false;
      let needsSetup = false;
      let setupHelpText: string | undefined;

      // Create user messages + tool calls for setup/cleanup scripts
      const allEntries = Object.values(executionProcessState)
        .sort(
          (a, b) =>
            new Date(
              a.executionProcess.created_at as unknown as string
            ).getTime() -
            new Date(
              b.executionProcess.created_at as unknown as string
            ).getTime()
        )
        .flatMap((p, index) => {
          const entries: PatchTypeWithKey[] = [];
          if (
            p.executionProcess.executor_action.typ.type ===
              'CodingAgentInitialRequest' ||
            p.executionProcess.executor_action.typ.type ===
              'CodingAgentFollowUpRequest'
          ) {
            // New user message
            const userNormalizedEntry: NormalizedEntry = {
              entry_type: {
                type: 'user_message',
              },
              content: p.executionProcess.executor_action.typ.prompt,
              timestamp: null,
              metadata: null,
            };
            const userPatch: PatchType = {
              type: 'NORMALIZED_ENTRY',
              content: userNormalizedEntry,
            };
            const userPatchTypeWithKey = patchWithKey(
              userPatch,
              p.executionProcess.id,
              'user'
            );
            entries.push(userPatchTypeWithKey);

            // Remove all coding agent added user messages, replace with our custom one
            const entriesExcludingUser = p.entries.filter(
              (e) =>
                e.type !== 'NORMALIZED_ENTRY' ||
                e.content.entry_type.type !== 'user_message'
            );

            const hasPendingApprovalEntry = entriesExcludingUser.some(
              (entry) => {
                if (entry.type !== 'NORMALIZED_ENTRY') return false;
                const entryType = entry.content.entry_type;
                return (
                  entryType.type === 'tool_use' &&
                  entryType.status.status === 'pending_approval'
                );
              }
            );

            if (hasPendingApprovalEntry) {
              hasPendingApproval = true;
            }

            entries.push(...entriesExcludingUser);

            const liveProcessStatus = getLiveExecutionProcess(
              p.executionProcess.id
            )?.status;
            const isProcessRunning =
              liveProcessStatus === ExecutionProcessStatus.running;
            const processFailedOrKilled =
              liveProcessStatus === ExecutionProcessStatus.failed ||
              liveProcessStatus === ExecutionProcessStatus.killed;

            if (isProcessRunning) {
              hasRunningProcess = true;
            }

            if (
              processFailedOrKilled &&
              index === Object.keys(executionProcessState).length - 1
            ) {
              lastProcessFailedOrKilled = true;

              // Check if this failed process has a SetupRequired entry
              const hasSetupRequired = entriesExcludingUser.some((entry) => {
                if (entry.type !== 'NORMALIZED_ENTRY') return false;
                if (
                  entry.content.entry_type.type === 'error_message' &&
                  entry.content.entry_type.error_type.type === 'setup_required'
                ) {
                  setupHelpText = entry.content.content;
                  return true;
                }
                return false;
              });

              if (hasSetupRequired) {
                needsSetup = true;
              }
            }

            if (isProcessRunning && !hasPendingApprovalEntry) {
              entries.push(makeLoadingPatch(p.executionProcess.id));
            }
          } else if (
            p.executionProcess.executor_action.typ.type === 'ScriptRequest'
          ) {
            // Add setup and cleanup script as a tool call
            let toolName = '';
            switch (p.executionProcess.executor_action.typ.context) {
              case 'SetupScript':
                toolName = 'Setup Script';
                break;
              case 'CleanupScript':
                toolName = 'Cleanup Script';
                break;
              case 'ToolInstallScript':
                toolName = 'Tool Install Script';
                break;
              default:
                return [];
            }

            const executionProcess = getLiveExecutionProcess(
              p.executionProcess.id
            );

            if (executionProcess?.status === ExecutionProcessStatus.running) {
              hasRunningProcess = true;
            }

            if (
              (executionProcess?.status === ExecutionProcessStatus.failed ||
                executionProcess?.status === ExecutionProcessStatus.killed) &&
              index === Object.keys(executionProcessState).length - 1
            ) {
              lastProcessFailedOrKilled = true;
            }

            const exitCode = Number(executionProcess?.exit_code) || 0;
            const exit_status: CommandExitStatus | null =
              executionProcess?.status === 'running'
                ? null
                : {
                    type: 'exit_code',
                    code: exitCode,
                  };

            const toolStatus: ToolStatus =
              executionProcess?.status === ExecutionProcessStatus.running
                ? { status: 'created' }
                : exitCode === 0
                  ? { status: 'success' }
                  : { status: 'failed' };

            const output = p.entries.map((line) => line.content).join('\n');

            const toolNormalizedEntry: NormalizedEntry = {
              entry_type: {
                type: 'tool_use',
                tool_name: toolName,
                action_type: {
                  action: 'command_run',
                  command: p.executionProcess.executor_action.typ.script,
                  result: {
                    output,
                    exit_status,
                  },
                },
                status: toolStatus,
              },
              content: toolName,
              timestamp: null,
              metadata: null,
            };
            const toolPatch: PatchType = {
              type: 'NORMALIZED_ENTRY',
              content: toolNormalizedEntry,
            };
            const toolPatchWithKey: PatchTypeWithKey = patchWithKey(
              toolPatch,
              p.executionProcess.id,
              0
            );

            entries.push(toolPatchWithKey);
          }

          return entries;
        });

      // Apply grouping to consecutive non-message entries
      const groupedEntries = groupConsecutiveNonMessages(allEntries);

      // Emit the next action bar if no process running
      if (!hasRunningProcess && !hasPendingApproval) {
        groupedEntries.push(
          nextActionPatch(
            lastProcessFailedOrKilled,
            Object.keys(executionProcessState).length,
            needsSetup,
            setupHelpText
          )
        );
      }

      return groupedEntries;
    },
    []
  );

  const emitEntries = useCallback(
    (
      executionProcessState: ExecutionProcessStateStore,
      addEntryType: AddEntryType,
      loading: boolean
    ) => {
      const entries = flattenEntriesForEmit(executionProcessState);
      onEntriesUpdatedRef.current?.(entries, addEntryType, loading);
      updateHasMoreHistory();
    },
    [flattenEntriesForEmit, updateHasMoreHistory]
  );

  // This emits its own events as they are streamed
  const loadRunningAndEmit = useCallback(
    (executionProcess: ExecutionProcess): Promise<void> => {
      return new Promise((resolve, reject) => {
        let url = '';
        if (executionProcess.executor_action.typ.type === 'ScriptRequest') {
          url = `/api/execution-processes/${executionProcess.id}/raw-logs/ws`;
        } else {
          url = `/api/execution-processes/${executionProcess.id}/normalized-logs/ws`;
        }
        const controller = streamJsonPatchEntries<PatchType>(url, {
          onEntries(entries) {
            const patchesWithKey = entries.map((entry, index) =>
              patchWithKey(entry, executionProcess.id, index)
            );
            mergeIntoDisplayed((state) => {
              state[executionProcess.id] = {
                executionProcess,
                entries: patchesWithKey,
                hasMore: false,
                nextBeforeIndex: null,
              };
            });
            emitEntries(displayedExecutionProcesses.current, 'running', false);
          },
          onFinished: () => {
            emitEntries(displayedExecutionProcesses.current, 'running', false);
            controller.close();
            resolve();
          },
          onError: () => {
            controller.close();
            reject();
          },
        });
      });
    },
    [emitEntries]
  );

  // Sometimes it can take a few seconds for the stream to start, wrap the loadRunningAndEmit method
  const loadRunningAndEmitWithBackoff = useCallback(
    async (executionProcess: ExecutionProcess) => {
      for (let i = 0; i < 20; i++) {
        try {
          await loadRunningAndEmit(executionProcess);
          break;
        } catch (_) {
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
      }
    },
    [loadRunningAndEmit]
  );

  const loadInitialEntries = useCallback(
    async (): Promise<ExecutionProcessStateStore> => {
      const localDisplayedExecutionProcesses: ExecutionProcessStateStore = {};

      if (!executionProcesses?.current) return localDisplayedExecutionProcesses;

      for (const executionProcess of [
        ...executionProcesses.current,
      ].reverse()) {
        if (executionProcess.status === ExecutionProcessStatus.running) {
          continue;
        }

        const page = await loadHistoricEntriesPage(executionProcess);

        localDisplayedExecutionProcesses[executionProcess.id] = {
          executionProcess,
          entries: page.entries,
          hasMore: page.hasMore,
          nextBeforeIndex: page.nextBeforeIndex,
        };

        if (
          flattenEntries(localDisplayedExecutionProcesses).length >
          MIN_INITIAL_ENTRIES
        ) {
          break;
        }
      }

      return localDisplayedExecutionProcesses;
    },
    [executionProcesses, loadHistoricEntriesPage]
  );

  const loadMoreHistory = useCallback(async () => {
    if (loadingMoreRef.current) return;
    loadingMoreRef.current = true;
    setIsLoadingMore(true);

    try {
      const processes = executionProcesses.current ?? [];
      if (processes.length === 0) return;

      const sorted = [...processes].sort(
        (a, b) =>
          new Date(a.created_at as unknown as string).getTime() -
          new Date(b.created_at as unknown as string).getTime()
      );
      const loadedIds = new Set(
        Object.keys(displayedExecutionProcesses.current)
      );
      const earliestLoadedIndex = sorted.findIndex((p) =>
        loadedIds.has(p.id)
      );

      if (earliestLoadedIndex === -1) return;

      const earliestProcess = sorted[earliestLoadedIndex];
      const earliestState =
        displayedExecutionProcesses.current[earliestProcess.id];

      let targetProcess: ExecutionProcess | undefined;
      let beforeIndex: number | undefined;

      if (earliestState?.hasMore && earliestState.nextBeforeIndex !== null) {
        targetProcess = earliestProcess;
        beforeIndex = earliestState.nextBeforeIndex ?? undefined;
      } else if (earliestLoadedIndex > 0) {
        targetProcess = sorted[earliestLoadedIndex - 1];
      }

      if (!targetProcess) return;

      const page = await loadHistoricEntriesPage(targetProcess, beforeIndex);

      mergeIntoDisplayed((state) => {
        const existing = state[targetProcess.id];
        if (existing) {
          state[targetProcess.id] = {
            ...existing,
            entries: [...page.entries, ...existing.entries],
            hasMore: page.hasMore,
            nextBeforeIndex: page.nextBeforeIndex,
          };
        } else {
          state[targetProcess.id] = {
            executionProcess: targetProcess,
            entries: page.entries,
            hasMore: page.hasMore,
            nextBeforeIndex: page.nextBeforeIndex,
          };
        }
      });

      emitEntries(displayedExecutionProcesses.current, 'historic', false);
    } finally {
      loadingMoreRef.current = false;
      setIsLoadingMore(false);
    }
  }, [emitEntries, loadHistoricEntriesPage]);

  const ensureProcessVisible = useCallback((p: ExecutionProcess) => {
    mergeIntoDisplayed((state) => {
      if (!state[p.id]) {
        state[p.id] = {
          executionProcess: {
            id: p.id,
            created_at: p.created_at,
            updated_at: p.updated_at,
            executor_action: p.executor_action,
          },
          entries: [],
          hasMore: false,
          nextBeforeIndex: null,
        };
      }
    });
  }, []);

  const idListKey = useMemo(
    () => executionProcessesRaw?.map((p) => p.id).join(','),
    [executionProcessesRaw]
  );

  const idStatusKey = useMemo(
    () => executionProcessesRaw?.map((p) => `${p.id}:${p.status}`).join(','),
    [executionProcessesRaw]
  );

  // Initial load when attempt changes
  useEffect(() => {
    let cancelled = false;
    (async () => {
      // Already loaded initial entries
      if (loadedInitialEntries.current) return;

      // Still waiting for execution processes WebSocket to connect
      if (isExecutionProcessesLoading || !isExecutionProcessesConnected) return;

      // Execution processes have been loaded (may be empty for new conversations)
      if (executionProcesses?.current.length === 0) {
        // No execution processes - emit empty state with loading=false
        emitEntries(displayedExecutionProcesses.current, 'initial', false);
        loadedInitialEntries.current = true;
        return;
      }

      // Initial entries
      const allInitialEntries = await loadInitialEntries();
      if (cancelled) return;
      mergeIntoDisplayed((state) => {
        Object.assign(state, allInitialEntries);
      });
      emitEntries(displayedExecutionProcesses.current, 'initial', false);
      loadedInitialEntries.current = true;
    })();
    return () => {
      cancelled = true;
    };
  }, [
    modeId,
    idListKey,
    isExecutionProcessesLoading,
    isExecutionProcessesConnected,
    loadInitialEntries,
    emitEntries,
  ]); // include idListKey so new processes trigger reload

  useEffect(() => {
    const activeProcesses = getActiveAgentProcesses();
    if (activeProcesses.length === 0) return;

    for (const activeProcess of activeProcesses) {
      if (!displayedExecutionProcesses.current[activeProcess.id]) {
        const runningOrInitial =
          Object.keys(displayedExecutionProcesses.current).length > 1
            ? 'running'
            : 'initial';
        ensureProcessVisible(activeProcess);
        emitEntries(
          displayedExecutionProcesses.current,
          runningOrInitial,
          false
        );
      }

      if (
        activeProcess.status === ExecutionProcessStatus.running &&
        !streamingProcessIdsRef.current.has(activeProcess.id)
      ) {
        streamingProcessIdsRef.current.add(activeProcess.id);
        loadRunningAndEmitWithBackoff(activeProcess).finally(() => {
          streamingProcessIdsRef.current.delete(activeProcess.id);
        });
      }
    }
  }, [
    modeId,
    idStatusKey,
    emitEntries,
    ensureProcessVisible,
    loadRunningAndEmitWithBackoff,
  ]);

  // If an execution process is removed, remove it from the state
  useEffect(() => {
    if (!executionProcessesRaw) return;

    const removedProcessIds = Object.keys(
      displayedExecutionProcesses.current
    ).filter((id) => !executionProcessesRaw.some((p) => p.id === id));

    if (removedProcessIds.length > 0) {
      mergeIntoDisplayed((state) => {
        removedProcessIds.forEach((id) => {
          delete state[id];
        });
      });
    }
  }, [modeId, idListKey, executionProcessesRaw]);

  // Reset state when mode changes
  useEffect(() => {
    displayedExecutionProcesses.current = {};
    loadedInitialEntries.current = false;
    streamingProcessIdsRef.current.clear();
    loadingMoreRef.current = false;
    setHasMoreHistory(false);
    setIsLoadingMore(false);
    emitEntries(displayedExecutionProcesses.current, 'initial', true);
  }, [modeId, emitEntries]);

  return {
    loadMoreHistory,
    hasMoreHistory,
    isLoadingMore,
  };
};
