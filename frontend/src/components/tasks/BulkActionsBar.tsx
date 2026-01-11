import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import { useProject } from '@/contexts/ProjectContext';
import { BulkCreateAttemptsDialog, BulkAssignGroupDialog } from '@/components/dialogs';
import { Layers, X } from 'lucide-react';

export function BulkActionsBar() {
  const { t } = useTranslation('tasks');
  const { selectedCount, clearSelection, getSelectedIds } = useTaskSelection();
  const { projectId } = useProject();

  if (selectedCount === 0) {
    return null;
  }

  const handleCreateAttempts = () => {
    BulkCreateAttemptsDialog.show({ taskIds: getSelectedIds() });
  };

  const handleAssignToGroup = () => {
    if (!projectId) return;
    BulkAssignGroupDialog.show({ projectId, taskIds: getSelectedIds() });
  };

  return createPortal(
    <Card className="fixed bottom-8 left-1/2 -translate-x-1/2 z-40 flex items-center gap-4 px-4 py-3 shadow-lg border rounded-lg">
      <span className="text-sm font-medium">
        {t('bulkActions.selectedCount', { count: selectedCount })}
      </span>
      <div className="flex items-center gap-2">
        <Button variant="default" size="sm" onClick={handleCreateAttempts}>
          {t('bulkActions.createAttempts')}
        </Button>
        <Button variant="secondary" size="sm" onClick={handleAssignToGroup}>
          <Layers className="h-4 w-4 mr-1" />
          {t('bulkActions.assignToGroup')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={clearSelection}
          className="text-muted-foreground"
        >
          <X className="h-4 w-4 mr-1" />
          {t('bulkActions.clearSelection')}
        </Button>
      </div>
    </Card>,
    document.body
  );
}
