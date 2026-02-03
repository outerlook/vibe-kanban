import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Plus } from 'lucide-react';
import { useTaskGroups, useAssignTasksToGroup } from '@/hooks/useTaskGroups';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import { TaskGroupFormDialog } from './TaskGroupFormDialog';
import GroupSelector from '@/components/tasks/GroupSelector';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';

export interface BulkAssignGroupDialogProps {
  projectId: string;
  taskIds: string[];
}

const BulkAssignGroupDialogImpl = NiceModal.create<BulkAssignGroupDialogProps>(
  ({ projectId, taskIds }) => {
    const modal = useModal();
    const { t } = useTranslation(['tasks', 'common']);
    const { clearSelection } = useTaskSelection();

    const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    const { data: groups = [], isLoading: isLoadingGroups } = useTaskGroups(
      projectId,
      { enabled: modal.visible }
    );

    const assignMutation = useAssignTasksToGroup();

    const isLoading = assignMutation.isPending;
    const hasGroups = groups.length > 0;

    useEffect(() => {
      if (!modal.visible) {
        setSelectedGroupId(null);
        setError(null);
      }
    }, [modal.visible]);

    const canSubmit = !!selectedGroupId && !isLoading && !isLoadingGroups;

    const handleGroupSelect = (groupId: string | null) => {
      setSelectedGroupId(groupId);
      setError(null);
    };

    const handleSubmit = async () => {
      if (!selectedGroupId) {
        setError(t('bulkAssignGroupDialog.errors.selectGroup'));
        return;
      }

      setError(null);

      try {
        await assignMutation.mutateAsync({
          groupId: selectedGroupId,
          taskIds,
          projectId,
        });
        clearSelection();
        modal.hide();
      } catch (err) {
        console.error('Failed to assign tasks to group:', err);
        setError(t('bulkAssignGroupDialog.errors.assignFailed'));
      }
    };

    const handleOpenChange = (open: boolean) => {
      if (!open && !isLoading) {
        modal.hide();
      }
    };

    const selectedGroup = groups.find((g) => g.id === selectedGroupId);

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>
              {t('bulkAssignGroupDialog.title', { count: taskIds.length })}
            </DialogTitle>
            <DialogDescription>
              {t('bulkAssignGroupDialog.description', {
                count: taskIds.length,
              })}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label>{t('bulkAssignGroupDialog.groupLabel')}</Label>
              {isLoadingGroups ? (
                <div className="text-sm text-muted-foreground">
                  {t('bulkAssignGroupDialog.loadingGroups')}
                </div>
              ) : (
                <div className="flex gap-2">
                  <GroupSelector
                    groups={groups}
                    selectedGroupId={selectedGroupId}
                    onGroupSelect={handleGroupSelect}
                    placeholder={t('bulkAssignGroupDialog.selectPlaceholder')}
                    disabled={isLoading}
                    className="flex-1"
                  />
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    className="h-9 w-9 shrink-0"
                    onClick={() =>
                      TaskGroupFormDialog.show({ mode: 'create', projectId })
                    }
                    disabled={isLoading}
                    aria-label={t('bulkAssignGroupDialog.createNew')}
                  >
                    <Plus className="h-4 w-4" />
                  </Button>
                </div>
              )}
              {!hasGroups && !isLoadingGroups && (
                <p className="text-xs text-muted-foreground">
                  {t('bulkAssignGroupDialog.noGroupsHint')}
                </p>
              )}
            </div>

            {selectedGroup && (
              <div className="text-sm text-muted-foreground">
                {t('bulkAssignGroupDialog.assigningTo', {
                  count: taskIds.length,
                  groupName: selectedGroup.name,
                })}
              </div>
            )}

            {error && <div className="text-sm text-destructive">{error}</div>}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={isLoading}
            >
              {t('common:buttons.cancel')}
            </Button>
            <Button onClick={handleSubmit} disabled={!canSubmit}>
              {isLoading
                ? t('bulkAssignGroupDialog.assigning')
                : t('bulkAssignGroupDialog.assign', { count: taskIds.length })}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const BulkAssignGroupDialog = defineModal<
  BulkAssignGroupDialogProps,
  void
>(BulkAssignGroupDialogImpl);
