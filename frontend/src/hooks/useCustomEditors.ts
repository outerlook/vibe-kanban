import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { customEditorsApi } from '@/lib/api';
import type {
  CreateCustomEditorRequest,
  CustomEditor,
  CustomEditorResponse,
  UpdateCustomEditorRequest,
} from 'shared/types';

const customEditorsKey = ['customEditors'] as const;

export type CustomEditorWithAvailability = CustomEditor & { available: boolean };

export function useCustomEditors() {
  const { data, isLoading, error } = useQuery<CustomEditorWithAvailability[]>({
    queryKey: customEditorsKey,
    queryFn: async () => {
      const response = await customEditorsApi.list();
      return response.editors.map((editor: CustomEditorResponse) => ({
        id: editor.id,
        name: editor.name,
        command: editor.command,
        argument: editor.argument,
        icon: editor.icon,
        created_at: editor.created_at,
        available: editor.available,
      }));
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
