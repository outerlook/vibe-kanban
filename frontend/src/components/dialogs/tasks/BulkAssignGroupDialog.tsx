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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Plus } from 'lucide-react';
import {
  useTaskGroups,
  useAssignTasksToGroup,
} from '@/hooks/useTaskGroups';
import { TaskGroupFormDialog } from './TaskGroupFormDialog';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';

const CREATE_NEW_VALUE = '__create_new__';

export interface BulkAssignGroupDialogProps {
  projectId: string;
  taskIds: string[];
}

const BulkAssignGroupDialogImpl = NiceModal.create<BulkAssignGroupDialogProps>(
  ({ projectId, taskIds }) => {
    const modal = useModal();
    const { t } = useTranslation(['tasks', 'common']);

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

    const handleSelectChange = async (value: string) => {
      if (value === CREATE_NEW_VALUE) {
        await TaskGroupFormDialog.show({ mode: 'create', projectId });
        return;
      }
      setSelectedGroupId(value);
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
              {t('bulkAssignGroupDialog.description', { count: taskIds.length })}
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
                <Select
                  value={selectedGroupId ?? ''}
                  onValueChange={handleSelectChange}
                >
                  <SelectTrigger>
                    <SelectValue
                      placeholder={t('bulkAssignGroupDialog.selectPlaceholder')}
                    />
                  </SelectTrigger>
                  <SelectContent>
                    {groups.map((group) => (
                      <SelectItem key={group.id} value={group.id}>
                        {group.name}
                        {group.base_branch && (
                          <span className="text-muted-foreground ml-2">
                            ({group.base_branch})
                          </span>
                        )}
                      </SelectItem>
                    ))}
                    <SelectItem value={CREATE_NEW_VALUE}>
                      <span className="flex items-center gap-2">
                        <Plus className="h-4 w-4" />
                        {t('bulkAssignGroupDialog.createNew')}
                      </span>
                    </SelectItem>
                  </SelectContent>
                </Select>
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
