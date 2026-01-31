import { createContext, useContext } from 'react';

export const TaskAttemptContext = createContext<string | undefined>(undefined);

export function useTaskAttemptId() {
  return useContext(TaskAttemptContext);
}

export const TaskContext = createContext<string | undefined>(undefined);

export function useTaskId() {
  return useContext(TaskContext);
}

export const ConversationContext = createContext<string | undefined>(undefined);

export function useConversationId() {
  return useContext(ConversationContext);
}

// Local images metadata for rendering uploaded images before they're saved
export type LocalImageMetadata = {
  path: string; // ".vibe-images/uuid.png"
  proxy_url: string; // "/api/images/{id}/file"
  file_name: string;
  size_bytes: number;
  format: string;
};

export const LocalImagesContext = createContext<LocalImageMetadata[]>([]);

export function useLocalImages() {
  return useContext(LocalImagesContext);
}
