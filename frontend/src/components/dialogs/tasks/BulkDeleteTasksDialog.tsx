import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import { useTaskMutations } from '@/hooks';
import { defineModal } from '@/lib/modals';

export interface BulkDeleteTasksDialogProps {
  projectId: string;
  taskIds: string[];
}

const BulkDeleteTasksDialogImpl = NiceModal.create<BulkDeleteTasksDialogProps>(
  ({ projectId, taskIds }) => {
    const modal = useModal();
    const { t } = useTranslation(['tasks', 'common']);
    const { clearSelection } = useTaskSelection();
    const { bulkDeleteTasks } = useTaskMutations(projectId);
    const [error, setError] = useState<string | null>(null);

    const isDeleting = bulkDeleteTasks.isPending;

    useEffect(() => {
      if (!modal.visible) {
        setError(null);
      }
    }, [modal.visible]);

    const handleClose = () => {
      if (isDeleting) return;
      modal.hide();
      modal.reject();
    };

    const handleConfirm = async () => {
      setError(null);

      try {
        await bulkDeleteTasks.mutateAsync({ taskIds });
        clearSelection();
        modal.resolve();
        modal.hide();
      } catch (err) {
        setError(
          err instanceof Error
            ? err.message
            : t('bulkDeleteDialog.errors.deleteFailed')
        );
      }
    };

    return (
      <Dialog open={modal.visible} onOpenChange={(open) => !open && handleClose()}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>
              {t('bulkDeleteDialog.title', { count: taskIds.length })}
            </DialogTitle>
            <DialogDescription>
              {t('bulkDeleteDialog.description', { count: taskIds.length })}
            </DialogDescription>
          </DialogHeader>

          <Alert variant="destructive">
            <strong>{t('bulkDeleteDialog.warningTitle')}</strong>{' '}
            {t('bulkDeleteDialog.warning')}
          </Alert>

          {error && <Alert variant="destructive">{error}</Alert>}

          <DialogFooter>
            <Button variant="outline" onClick={handleClose} disabled={isDeleting}>
              {t('common:buttons.cancel')}
            </Button>
            <Button
              variant="destructive"
              onClick={handleConfirm}
              disabled={isDeleting}
            >
              {isDeleting
                ? t('bulkDeleteDialog.deleting')
                : t('bulkDeleteDialog.confirm', { count: taskIds.length })}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const BulkDeleteTasksDialog = defineModal<BulkDeleteTasksDialogProps, void>(
  BulkDeleteTasksDialogImpl
);
