import { useCallback, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  conversationsApi,
  executionProcessesApi,
  type CreateConversationRequest,
  type UpdateConversationRequest,
  type SendConversationMessageRequest,
  type ListConversationsParams,
} from '@/lib/api';
import type { ConversationMessage, ConversationWithMessages, SendMessageResponse } from 'shared/types';

export const conversationKeys = {
  all: ['conversations'] as const,
  lists: () => [...conversationKeys.all, 'list'] as const,
  list: (projectId: string, params?: ListConversationsParams) =>
    [...conversationKeys.lists(), projectId, params ?? {}] as const,
  details: () => [...conversationKeys.all, 'detail'] as const,
  detail: (id: string) => [...conversationKeys.details(), id] as const,
  messages: (id: string) => [...conversationKeys.all, 'messages', id] as const,
  executions: (id: string) =>
    [...conversationKeys.all, 'executions', id] as const,
};

export interface UseConversationsOptions {
  worktreePath?: string;
}

export function useConversations(
  projectId: string | undefined,
  options?: UseConversationsOptions
) {
  const params: ListConversationsParams | undefined = options?.worktreePath
    ? { worktree_path: options.worktreePath }
    : undefined;

  return useQuery({
    queryKey: conversationKeys.list(projectId ?? '', params),
    queryFn: () => conversationsApi.list(projectId!, params),
    enabled: !!projectId,
    staleTime: 30_000,
  });
}

export function useConversation(conversationId: string | undefined) {
  return useQuery({
    queryKey: conversationKeys.detail(conversationId ?? ''),
    queryFn: () => conversationsApi.get(conversationId!),
    enabled: !!conversationId,
    staleTime: 10_000,
  });
}

export function useConversationExecutions(conversationId: string | undefined) {
  return useQuery({
    queryKey: conversationKeys.executions(conversationId ?? ''),
    queryFn: () => conversationsApi.getExecutions(conversationId!),
    enabled: !!conversationId,
    staleTime: 5_000,
    refetchInterval: 5_000,
  });
}

export interface UseConversationMessagesResult {
  messages: ConversationMessage[];
  isLoading: boolean;
  error: Error | null;
  hasMore: boolean;
  isLoadingMore: boolean;
  loadMore: () => void;
  addMessage: (message: ConversationMessage) => void;
}

export function useConversationMessages(
  conversationId: string | undefined
): UseConversationMessagesResult {
  const queryClient = useQueryClient();
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [initialLoadDone, setInitialLoadDone] = useState(false);

  // Initial fetch
  const { isLoading, error } = useQuery({
    queryKey: conversationKeys.messages(conversationId ?? ''),
    queryFn: async () => {
      const page = await conversationsApi.getMessages(conversationId!);
      setMessages(page.messages);
      setNextCursor(page.next_cursor);
      setHasMore(page.has_more);
      setInitialLoadDone(true);
      return page;
    },
    enabled: !!conversationId,
    staleTime: 10_000,
  });

  // Reset state when conversationId changes
  const prevConversationIdRef = useState<string | undefined>(undefined);
  if (prevConversationIdRef[0] !== conversationId) {
    prevConversationIdRef[1](conversationId);
    if (conversationId !== prevConversationIdRef[0]) {
      setMessages([]);
      setNextCursor(null);
      setHasMore(false);
      setInitialLoadDone(false);
    }
  }

  const loadMore = useCallback(() => {
    if (!conversationId || !hasMore || isLoadingMore || !nextCursor) {
      return;
    }

    setIsLoadingMore(true);

    conversationsApi
      .getMessages(conversationId, { cursor: nextCursor })
      .then((page) => {
        setMessages((prev) => [...prev, ...page.messages]);
        setNextCursor(page.next_cursor);
        setHasMore(page.has_more);
      })
      .catch((err) => {
        console.error('Failed to load more messages:', err);
      })
      .finally(() => {
        setIsLoadingMore(false);
      });
  }, [conversationId, hasMore, isLoadingMore, nextCursor]);

  // Add a new message (used for WebSocket updates)
  const addMessage = useCallback((message: ConversationMessage) => {
    setMessages((prev) => {
      // Avoid duplicates
      if (prev.some((m) => m.id === message.id)) {
        return prev;
      }
      return [...prev, message];
    });
    // Also update the conversation detail cache if it exists
    queryClient.setQueryData(
      conversationKeys.detail(message.conversation_session_id),
      (old: ConversationWithMessages | undefined) => {
        if (!old) return undefined;
        if (old.messages.some((m) => m.id === message.id)) {
          return old;
        }
        return {
          ...old,
          messages: [...old.messages, message],
        };
      }
    );
  }, [queryClient]);

  return {
    messages,
    isLoading: isLoading || !initialLoadDone,
    error: error ?? null,
    hasMore,
    isLoadingMore,
    loadMore,
    addMessage,
  };
}

export function useCreateConversation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      projectId,
      data,
    }: {
      projectId: string;
      data: CreateConversationRequest;
    }) => conversationsApi.create(projectId, data),
    onSuccess: (result, { projectId }) => {
      queryClient.invalidateQueries({
        queryKey: conversationKeys.list(projectId),
      });
      queryClient.setQueryData(
        conversationKeys.detail(result.session.id),
        {
          ...result.session,
          messages: [result.initial_message],
        } as ConversationWithMessages
      );
    },
  });
}

export function useUpdateConversation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      conversationId,
      data,
    }: {
      conversationId: string;
      data: UpdateConversationRequest;
    }) => conversationsApi.update(conversationId, data),
    onSuccess: (result) => {
      queryClient.setQueryData(
        conversationKeys.detail(result.id),
        (old: ConversationWithMessages | undefined) =>
          old ? { ...old, ...result } : undefined
      );
      queryClient.invalidateQueries({
        queryKey: conversationKeys.lists(),
      });
    },
  });
}

export function useDeleteConversation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (conversationId: string) =>
      conversationsApi.delete(conversationId),
    onSuccess: (_, conversationId) => {
      queryClient.removeQueries({
        queryKey: conversationKeys.detail(conversationId),
      });
      queryClient.invalidateQueries({
        queryKey: conversationKeys.lists(),
      });
    },
  });
}

export function useSendMessage() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      conversationId,
      data,
    }: {
      conversationId: string;
      data: SendConversationMessageRequest;
    }) => conversationsApi.sendMessage(conversationId, data),
    onSuccess: (result: SendMessageResponse, { conversationId }) => {
      queryClient.setQueryData(
        conversationKeys.detail(conversationId),
        (old: ConversationWithMessages | undefined) => {
          if (!old) return undefined;
          return {
            ...old,
            messages: [...old.messages, result.user_message],
          };
        }
      );
      // Invalidate the paginated messages query to refresh state
      queryClient.invalidateQueries({
        queryKey: conversationKeys.messages(conversationId),
      });
      queryClient.invalidateQueries({
        queryKey: conversationKeys.executions(conversationId),
      });
    },
  });
}

export function useStopConversationExecution(conversationId: string | undefined) {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: (executionProcessId: string) =>
      executionProcessesApi.stopExecutionProcess(executionProcessId),
    onSuccess: () => {
      if (conversationId) {
        queryClient.invalidateQueries({
          queryKey: conversationKeys.executions(conversationId),
        });
      }
    },
    onError: (error) => {
      console.error('Failed to stop execution:', error);
    },
  });

  return {
    stopExecution: mutation.mutate,
    isStopping: mutation.isPending,
  };
}
