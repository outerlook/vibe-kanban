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
import { useTaskGroups, useMergeTaskGroup } from '@/hooks/useTaskGroups';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import type { TaskGroup } from 'shared/types';

export interface MergeGroupDialogProps {
  sourceGroup: TaskGroup;
  projectId: string;
}

const MergeGroupDialogImpl = NiceModal.create<MergeGroupDialogProps>(
  ({ sourceGroup, projectId }) => {
    const modal = useModal();
    const { t } = useTranslation(['tasks', 'common']);

    const [selectedTargetId, setSelectedTargetId] = useState<string | null>(
      null
    );
    const [error, setError] = useState<string | null>(null);

    const { data: groups = [], isLoading: isLoadingGroups } = useTaskGroups(
      projectId,
      { enabled: modal.visible }
    );

    const mergeMutation = useMergeTaskGroup();

    const isLoading = mergeMutation.isPending;

    // Filter out the source group from available targets
    const availableTargets = groups.filter((g) => g.id !== sourceGroup.id);

    useEffect(() => {
      if (!modal.visible) {
        setSelectedTargetId(null);
        setError(null);
      }
    }, [modal.visible]);

    const canSubmit = !!selectedTargetId && !isLoading && !isLoadingGroups;

    const handleSelectChange = (value: string) => {
      setSelectedTargetId(value);
      setError(null);
    };

    const handleSubmit = async () => {
      if (!selectedTargetId) {
        setError(t('mergeGroupDialog.errors.selectTarget'));
        return;
      }

      setError(null);

      try {
        await mergeMutation.mutateAsync({
          sourceId: sourceGroup.id,
          targetId: selectedTargetId,
          projectId,
        });
        modal.hide();
      } catch (err) {
        console.error('Failed to merge task groups:', err);
        setError(t('mergeGroupDialog.errors.mergeFailed'));
      }
    };

    const handleOpenChange = (open: boolean) => {
      if (!open && !isLoading) {
        modal.hide();
      }
    };

    const selectedTarget = groups.find((g) => g.id === selectedTargetId);

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>{t('mergeGroupDialog.title')}</DialogTitle>
            <DialogDescription>
              {t('mergeGroupDialog.description', {
                sourceName: sourceGroup.name,
              })}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label>{t('mergeGroupDialog.targetLabel')}</Label>
              {isLoadingGroups ? (
                <div className="text-sm text-muted-foreground">
                  {t('mergeGroupDialog.loadingGroups')}
                </div>
              ) : availableTargets.length === 0 ? (
                <div className="text-sm text-muted-foreground">
                  {t('mergeGroupDialog.noTargetsAvailable')}
                </div>
              ) : (
                <Select
                  value={selectedTargetId ?? ''}
                  onValueChange={handleSelectChange}
                >
                  <SelectTrigger>
                    <SelectValue
                      placeholder={t('mergeGroupDialog.selectPlaceholder')}
                    />
                  </SelectTrigger>
                  <SelectContent>
                    {availableTargets.map((group) => (
                      <SelectItem key={group.id} value={group.id}>
                        {group.name}
                        {group.base_branch && (
                          <span className="text-muted-foreground ml-2">
                            ({group.base_branch})
                          </span>
                        )}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            </div>

            {selectedTarget && (
              <div className="text-sm text-muted-foreground">
                {t('mergeGroupDialog.confirmation', {
                  sourceName: sourceGroup.name,
                  targetName: selectedTarget.name,
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
            <Button
              onClick={handleSubmit}
              disabled={!canSubmit || availableTargets.length === 0}
            >
              {isLoading
                ? t('mergeGroupDialog.merging')
                : t('mergeGroupDialog.merge')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const MergeGroupDialog = defineModal<MergeGroupDialogProps, void>(
  MergeGroupDialogImpl
);
