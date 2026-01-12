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
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import BranchSelector from '@/components/tasks/BranchSelector';
import { useProjectRepos, useRepoBranches } from '@/hooks';
import {
  useCreateTaskGroup,
  useUpdateTaskGroup,
} from '@/hooks/useTaskGroups';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal, type SaveResult } from '@/lib/modals';
import type { TaskGroup } from 'shared/types';

export type TaskGroupFormDialogProps =
  | { mode: 'create'; projectId: string }
  | { mode: 'edit'; projectId: string; group: TaskGroup };

const TaskGroupFormDialogImpl = NiceModal.create<TaskGroupFormDialogProps>(
  (props) => {
    const modal = useModal();
    const { t } = useTranslation(['tasks', 'common']);
    const { projectId } = props;
    const group = props.mode === 'edit' ? props.group : undefined;

    const [name, setName] = useState('');
    const [baseBranch, setBaseBranch] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    const { data: repos = [], isLoading: isLoadingRepos } = useProjectRepos(
      projectId,
      { enabled: modal.visible }
    );

    // Use first repo for branch selection (task groups have a single base_branch)
    const primaryRepo = repos[0];
    const { data: branches = [], isLoading: isLoadingBranches } =
      useRepoBranches(primaryRepo?.id, {
        enabled: modal.visible && !!primaryRepo,
      });

    const createMutation = useCreateTaskGroup(projectId);
    const updateMutation = useUpdateTaskGroup();

    const isLoading = createMutation.isPending || updateMutation.isPending;
    const isLoadingInitial = isLoadingRepos || isLoadingBranches;

    // Initialize form state when dialog opens or group changes
    useEffect(() => {
      if (modal.visible) {
        if (group) {
          setName(group.name);
          setBaseBranch(group.base_branch);
        } else {
          setName('');
          setBaseBranch(null);
        }
        setError(null);
      }
    }, [modal.visible, group]);

    const canSubmit = !!name.trim() && !isLoading && !isLoadingInitial;

    const handleSubmit = async () => {
      const trimmedName = name.trim();
      if (!trimmedName) {
        setError(t('taskGroupFormDialog.errors.nameRequired'));
        return;
      }

      setError(null);

      try {
        if (props.mode === 'create') {
          await createMutation.mutateAsync({
            name: trimmedName,
            base_branch: baseBranch,
          });
        } else {
          await updateMutation.mutateAsync({
            groupId: props.group.id,
            data: {
              name: trimmedName,
              base_branch: baseBranch,
            },
          });
        }

        modal.resolve('saved' as SaveResult);
        modal.hide();
      } catch (err) {
        console.error('Failed to save task group:', err);
        setError(
          props.mode === 'create'
            ? t('taskGroupFormDialog.errors.createFailed')
            : t('taskGroupFormDialog.errors.updateFailed')
        );
      }
    };

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        modal.resolve('canceled' as SaveResult);
        modal.hide();
      }
    };

    const dialogTitle =
      props.mode === 'create'
        ? t('taskGroupFormDialog.createTitle')
        : t('taskGroupFormDialog.editTitle');

    const dialogDescription =
      props.mode === 'create'
        ? t('taskGroupFormDialog.createDescription')
        : t('taskGroupFormDialog.editDescription');

    const submitButtonText = isLoading
      ? t('taskGroupFormDialog.saving')
      : props.mode === 'create'
        ? t('taskGroupFormDialog.create')
        : t('taskGroupFormDialog.update');

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>{dialogTitle}</DialogTitle>
            <DialogDescription>{dialogDescription}</DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="group-name">
                {t('taskGroupFormDialog.nameLabel')}
              </Label>
              <Input
                id="group-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('taskGroupFormDialog.namePlaceholder')}
                autoFocus
              />
            </div>

            {repos.length > 0 && (
              <div className="space-y-2">
                <Label>{t('taskGroupFormDialog.baseBranchLabel')}</Label>
                {isLoadingBranches ? (
                  <div className="text-sm text-muted-foreground">
                    {t('taskGroupFormDialog.loadingBranches')}
                  </div>
                ) : (
                  <BranchSelector
                    branches={branches}
                    selectedBranch={baseBranch}
                    onBranchSelect={setBaseBranch}
                    placeholder={t('taskGroupFormDialog.selectBranch')}
                    repoId={primaryRepo?.id}
                  />
                )}
                <p className="text-xs text-muted-foreground">
                  {t('taskGroupFormDialog.baseBranchHint')}
                </p>
              </div>
            )}

            {error && (
              <div className="text-sm text-destructive">{error}</div>
            )}
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
              {submitButtonText}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const TaskGroupFormDialog = defineModal<
  TaskGroupFormDialogProps,
  SaveResult
>(TaskGroupFormDialogImpl);
