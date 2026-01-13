import { useCallback } from 'react';
import { projectsApi } from '@/lib/api';
import { ProjectEditorSelectionDialog } from '@/components/dialogs/projects/ProjectEditorSelectionDialog';
import type { Project } from 'shared/types';

export function useOpenProjectInEditor(
  project: Project | null,
  onShowEditorDialog?: () => void
) {
  return useCallback(
    async (editorId?: string) => {
      if (!project) return;

      try {
        const response = await projectsApi.openEditor(project.id, {
          editor_type: editorId ?? null,
          file_path: null,
        });

        // If a URL is returned, open it in a new window/tab
        if (response.url) {
          window.open(response.url, '_blank');
        }
      } catch (err) {
        console.error('Failed to open project in editor:', err);
        if (!editorId) {
          if (onShowEditorDialog) {
            onShowEditorDialog();
          } else {
            ProjectEditorSelectionDialog.show({
              selectedProject: project,
            });
          }
        }
      }
    },
    [project, onShowEditorDialog]
  );
}
