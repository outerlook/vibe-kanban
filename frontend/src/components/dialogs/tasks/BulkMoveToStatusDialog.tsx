import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { TASK_STATUSES } from '@/constants/taskStatuses';
import { useTaskMutations } from '@/hooks';
import { defineModal, getErrorMessage } from '@/lib/modals';
import { statusBoardColors, getTaskStatusLabel } from '@/utils/statusLabels';
import type { TaskStatus } from 'shared/types';

export interface BulkMoveToStatusDialogProps {
  projectId: string;
  taskIds: string[];
}

const BulkMoveToStatusDialogImpl =
  NiceModal.create<BulkMoveToStatusDialogProps>(({ projectId, taskIds }) => {
    const modal = useModal();
    const { t } = useTranslation(['tasks', 'common']);
    const { bulkUpdateTaskStatus } = useTaskMutations(projectId);

    const [selectedStatus, setSelectedStatus] = useState<TaskStatus | null>(
      null
    );
    const [error, setError] = useState<string | null>(null);

    const statusOptions = useMemo(
      () =>
        TASK_STATUSES.map((status) => ({
          value: status,
          label: getTaskStatusLabel(t, status),
          colorVar: statusBoardColors[status],
        })),
      [t]
    );

    const isLoading = bulkUpdateTaskStatus.isPending;
    const canSubmit = selectedStatus !== null && !isLoading;

    useEffect(() => {
      if (!modal.visible) {
        setSelectedStatus(null);
        setError(null);
      }
    }, [modal.visible]);

    const handleSubmit = async () => {
      if (!selectedStatus) {
        setError(t('bulkMoveToStatusDialog.errors.selectStatus'));
        return;
      }

      setError(null);

      try {
        await bulkUpdateTaskStatus.mutateAsync({
          taskIds,
          status: selectedStatus,
        });
        modal.resolve();
        modal.hide();
      } catch (err) {
        setError(
          getErrorMessage(err) ||
            t('bulkMoveToStatusDialog.errors.updateFailed')
        );
      }
    };

    const handleCancel = () => {
      modal.reject();
      modal.hide();
    };

    const handleOpenChange = (open: boolean) => {
      if (!open && !isLoading) {
        handleCancel();
      }
    };

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>
              {t('bulkMoveToStatusDialog.title', { count: taskIds.length })}
            </DialogTitle>
            <DialogDescription>
              {t('bulkMoveToStatusDialog.description', {
                count: taskIds.length,
              })}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="bulk-move-status-select">
                {t('bulkMoveToStatusDialog.statusLabel')}
              </Label>
              <Select
                value={selectedStatus ?? undefined}
                onValueChange={(value) => {
                  setSelectedStatus(value as TaskStatus);
                  setError(null);
                }}
                disabled={isLoading}
              >
                <SelectTrigger id="bulk-move-status-select">
                  <SelectValue
                    placeholder={t('bulkMoveToStatusDialog.selectPlaceholder')}
                  />
                </SelectTrigger>
                <SelectContent>
                  {statusOptions.map((status) => (
                    <SelectItem key={status.value} value={status.value}>
                      <span className="flex items-center gap-2">
                        <span
                          className="inline-block h-2.5 w-2.5 rounded-full"
                          style={{
                            backgroundColor: `var(${status.colorVar})`,
                          }}
                        />
                        <span>{status.label}</span>
                      </span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {selectedStatus && (
              <div className="text-sm text-muted-foreground">
                {t('bulkMoveToStatusDialog.movingTo', {
                  count: taskIds.length,
                  status: getTaskStatusLabel(t, selectedStatus),
                })}
              </div>
            )}

            {error && <Alert variant="destructive">{error}</Alert>}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isLoading}
            >
              {t('common:buttons.cancel')}
            </Button>
            <Button onClick={handleSubmit} disabled={!canSubmit}>
              {isLoading
                ? t('bulkMoveToStatusDialog.updating')
                : t('bulkMoveToStatusDialog.update', {
                    count: taskIds.length,
                  })}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  });

export const BulkMoveToStatusDialog = defineModal<
  BulkMoveToStatusDialogProps,
  void
>(BulkMoveToStatusDialogImpl);
