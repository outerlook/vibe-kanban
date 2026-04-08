import { useState } from 'react';
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
import { Alert } from '@/components/ui/alert';
import { useTaskMutations } from '@/hooks';
import type { TaskWithAttemptStatus } from 'shared/types';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';

export interface DeleteTaskConfirmationDialogProps {
  projectId: string;
  task?: TaskWithAttemptStatus;
  taskIds?: string[];
}

const DeleteTaskConfirmationDialogImpl =
  NiceModal.create<DeleteTaskConfirmationDialogProps>(
    ({ task, projectId, taskIds = [] }) => {
      const modal = useModal();
      const { t } = useTranslation(['tasks', 'common']);
      const { deleteTask, bulkDeleteTasks } = useTaskMutations(projectId);
      const [isDeleting, setIsDeleting] = useState(false);
      const [error, setError] = useState<string | null>(null);

      const isBulkDelete = taskIds.length > 0;
      const selectedCount = taskIds.length;

      const handleConfirmDelete = async () => {
        setIsDeleting(true);
        setError(null);

        try {
          if (isBulkDelete) {
            await bulkDeleteTasks.mutateAsync({ taskIds });
          } else if (task) {
            await deleteTask.mutateAsync(task.id);
          } else {
            throw new Error(t('deleteTaskDialog.errors.missingTarget'));
          }

          modal.resolve();
          modal.hide();
        } catch (err: unknown) {
          const errorMessage =
            err instanceof Error
              ? err.message
              : t('deleteTaskDialog.errors.deleteFailed');
          setError(errorMessage);
        } finally {
          setIsDeleting(false);
        }
      };

      const handleCancelDelete = () => {
        if (isDeleting) return;
        modal.reject();
        modal.hide();
      };

      return (
        <Dialog
          open={modal.visible}
          onOpenChange={(open) => !open && handleCancelDelete()}
        >
          <DialogContent>
            <DialogHeader>
              <DialogTitle>
                {isBulkDelete
                  ? t('deleteTaskDialog.bulkTitle', { count: selectedCount })
                  : t('deleteTaskDialog.title')}
              </DialogTitle>
              <DialogDescription>
                {isBulkDelete
                  ? t('deleteTaskDialog.bulkDescription', {
                      count: selectedCount,
                    })
                  : t('deleteTaskDialog.description', {
                      title: task?.title ?? '',
                    })}
              </DialogDescription>
            </DialogHeader>

            <Alert variant="destructive" className="mb-4">
              <strong>{t('deleteTaskDialog.warningLabel')}</strong>{' '}
              {isBulkDelete
                ? t('deleteTaskDialog.bulkWarning', { count: selectedCount })
                : t('deleteTaskDialog.warning')}
            </Alert>

            {error && (
              <Alert variant="destructive" className="mb-4">
                {error}
              </Alert>
            )}

            <DialogFooter>
              <Button
                variant="outline"
                onClick={handleCancelDelete}
                disabled={isDeleting}
                autoFocus
              >
                {t('common:buttons.cancel')}
              </Button>
              <Button
                variant="destructive"
                onClick={handleConfirmDelete}
                disabled={isDeleting}
              >
                {isDeleting
                  ? t('deleteTaskDialog.deleting')
                  : isBulkDelete
                    ? t('deleteTaskDialog.bulkDeleteAction')
                    : t('deleteTaskDialog.deleteAction')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      );
    }
  );

export const DeleteTaskConfirmationDialog = defineModal<
  DeleteTaskConfirmationDialogProps,
  void
>(DeleteTaskConfirmationDialogImpl);
