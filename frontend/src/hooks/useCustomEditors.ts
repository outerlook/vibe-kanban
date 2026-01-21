import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { customEditorsApi } from '@/lib/api';
import type {
  CreateCustomEditorRequest,
  CustomEditorResponse,
  UpdateCustomEditorRequest,
} from 'shared/types';

const customEditorsKey = ['customEditors'] as const;

export function useCustomEditors() {
  const { data, isLoading, error } = useQuery<CustomEditorResponse[]>({
    queryKey: customEditorsKey,
    queryFn: async () => {
      const response = await customEditorsApi.list();
      return response.editors;
    },
    staleTime: 30_000,
  });

  return { data, isLoading, error };
}

export function useCreateCustomEditor() {
  const queryClient = useQueryClient();

  return useMutation<CustomEditorResponse, unknown, CreateCustomEditorRequest>({
    mutationFn: (data) => customEditorsApi.create(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: customEditorsKey });
    },
  });
}

type UpdateCustomEditorPayload = { id: string } & UpdateCustomEditorRequest;

export function useUpdateCustomEditor() {
  const queryClient = useQueryClient();

  return useMutation<CustomEditorResponse, unknown, UpdateCustomEditorPayload>({
    mutationFn: ({ id, ...data }) => customEditorsApi.update(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: customEditorsKey });
    },
  });
}

export function useDeleteCustomEditor() {
  const queryClient = useQueryClient();

  return useMutation<void, unknown, string>({
    mutationFn: (editorId) => customEditorsApi.delete(editorId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: customEditorsKey });
    },
  });
}
