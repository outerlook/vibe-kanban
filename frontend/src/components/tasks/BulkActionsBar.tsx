import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import { useProject } from '@/contexts/ProjectContext';
import { useCanBulkCreateAttempts } from '@/hooks';
import {
  BulkCreateAttemptsDialog,
  BulkAssignGroupDialog,
  BulkMoveToStatusDialog,
  DeleteTaskConfirmationDialog,
} from '@/components/dialogs';
import { ArrowRightLeft, Layers, Trash2, X } from 'lucide-react';

export function BulkActionsBar() {
  const { t } = useTranslation('tasks');
  const { selectedCount, clearSelection, getSelectedIds } = useTaskSelection();
  const { projectId } = useProject();
  const selectedIds = getSelectedIds();
  const { hasMixedGroups, isLoading } = useCanBulkCreateAttempts(selectedIds);

  if (selectedCount === 0) {
    return null;
  }

  const isCreateAttemptsDisabled = hasMixedGroups && !isLoading;

  const handleCreateAttempts = () => {
    BulkCreateAttemptsDialog.show({ taskIds: getSelectedIds() });
  };

  const handleAssignToGroup = () => {
    if (!projectId) return;
    BulkAssignGroupDialog.show({ projectId, taskIds: getSelectedIds() });
  };

  const handleMoveToStatus = async () => {
    if (!projectId) return;

    try {
      await BulkMoveToStatusDialog.show({
        projectId,
        taskIds: getSelectedIds(),
      });
      clearSelection();
    } catch {
      // Dialog closed without completing the action.
    }
  };

  const handleBulkDelete = async () => {
    if (!projectId) return;

    try {
      await DeleteTaskConfirmationDialog.show({
        projectId,
        taskIds: getSelectedIds(),
      });
      clearSelection();
    } catch {
      // Dialog closed without completing the action.
    }
  };

  return createPortal(
    <Card className="fixed bottom-8 left-1/2 -translate-x-1/2 z-40 flex items-center gap-4 px-4 py-3 shadow-lg border rounded-lg">
      <span className="text-sm font-medium">
        {t('bulkActions.selectedCount', { count: selectedCount })}
      </span>
      <div className="flex items-center gap-2">
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <span>
                <Button
                  variant="default"
                  size="sm"
                  onClick={handleCreateAttempts}
                  disabled={isCreateAttemptsDisabled}
                >
                  {t('bulkActions.createAttempts')}
                </Button>
              </span>
            </TooltipTrigger>
            {isCreateAttemptsDisabled && (
              <TooltipContent>
                {t('bulkActions.mixedGroupsDisabled')}
              </TooltipContent>
            )}
          </Tooltip>
        </TooltipProvider>
        <Button variant="secondary" size="sm" onClick={handleAssignToGroup}>
          <Layers className="h-4 w-4 mr-1" />
          {t('bulkActions.assignToGroup')}
        </Button>
        <Button variant="secondary" size="sm" onClick={handleMoveToStatus}>
          <ArrowRightLeft className="h-4 w-4 mr-1" />
          {t('bulkActions.moveToStatus')}
        </Button>
        <Button variant="destructive" size="sm" onClick={handleBulkDelete}>
          <Trash2 className="h-4 w-4 mr-1" />
          {t('bulkActions.deleteTasks')}
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
