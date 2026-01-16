import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  conversationsApi,
  type CreateConversationRequest,
  type UpdateConversationRequest,
  type SendConversationMessageRequest,
} from '@/lib/api';
import type { ConversationWithMessages, SendMessageResponse } from 'shared/types';

export const conversationKeys = {
  all: ['conversations'] as const,
  lists: () => [...conversationKeys.all, 'list'] as const,
  list: (projectId: string) =>
    [...conversationKeys.lists(), projectId] as const,
  details: () => [...conversationKeys.all, 'detail'] as const,
  detail: (id: string) => [...conversationKeys.details(), id] as const,
  executions: (id: string) =>
    [...conversationKeys.all, 'executions', id] as const,
};

export function useConversations(projectId: string | undefined) {
  return useQuery({
    queryKey: conversationKeys.list(projectId ?? ''),
    queryFn: () => conversationsApi.list(projectId!),
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
      queryClient.invalidateQueries({
        queryKey: conversationKeys.executions(conversationId),
      });
    },
  });
}
