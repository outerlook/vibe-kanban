import { useState, useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
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
import { Textarea } from '@/components/ui/textarea';
import BranchSelector from '@/components/tasks/BranchSelector';
import { WorkflowAssociationFields } from '@/components/tasks/WorkflowAssociationFields';
import {
  emptyWorkflowAssociationFormState,
  getAssociationAtScope,
  getWorkflowAssociationFormError,
  workflowAssociationFormToPayload,
  workflowAssociationToFormState,
} from '@/components/tasks/workflowAssociationHelpers';
import { useProjectRepos, useRepoBranches } from '@/hooks';
import { useTaskGroupMutations } from '@/hooks/useTaskGroups';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal, type SaveResult } from '@/lib/modals';
import { taskGroupsApi } from '@/lib/api';
import {
  useTaskGroupWorkflowAssociations,
  workflowAssociationKeys,
} from '@/hooks/useWorkflowAssociations';
import type { TaskGroup } from 'shared/types';

export type TaskGroupFormDialogProps =
  | { mode: 'create'; projectId: string }
  | { mode: 'edit'; projectId: string; group: TaskGroup };

const TaskGroupFormDialogImpl = NiceModal.create<TaskGroupFormDialogProps>(
  (props) => {
    const modal = useModal();
    const { t } = useTranslation(['tasks', 'common']);
    const queryClient = useQueryClient();
    const { projectId } = props;
    const group = props.mode === 'edit' ? props.group : undefined;

    const [name, setName] = useState('');
    const [description, setDescription] = useState('');
    const [baseBranch, setBaseBranch] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [workflowAssociation, setWorkflowAssociation] = useState(
      emptyWorkflowAssociationFormState
    );

    const { data: repos = [], isLoading: isLoadingRepos } = useProjectRepos(
      projectId,
      { enabled: modal.visible }
    );
    const { data: workflowResolution } = useTaskGroupWorkflowAssociations(
      group?.id,
      { enabled: modal.visible && !!group }
    );

    // Use first repo for branch selection (task groups have a single base_branch)
    const primaryRepo = repos[0];
    const { data: branches = [], isLoading: isLoadingBranches } =
      useRepoBranches(primaryRepo?.id, {
        enabled: modal.visible && !!primaryRepo,
      });

    const { createTaskGroup, updateTaskGroup } =
      useTaskGroupMutations(projectId);

    const isLoading = createTaskGroup.isPending || updateTaskGroup.isPending;
    const isLoadingInitial = isLoadingRepos || isLoadingBranches;

    // Initialize form state when dialog opens or group changes
    useEffect(() => {
      if (modal.visible) {
        if (group) {
          setName(group.name);
          setDescription(group.description || '');
          setBaseBranch(group.base_branch);
        } else {
          setName('');
          setDescription('');
          setBaseBranch(null);
        }
        setWorkflowAssociation(
          workflowAssociationToFormState(
            getAssociationAtScope(workflowResolution, 'task_group_default')
          )
        );
        setError(null);
      }
    }, [modal.visible, group, workflowResolution]);

    const workflowAssociationError =
      getWorkflowAssociationFormError(workflowAssociation);
    const canSubmit =
      !!name.trim() &&
      !isLoading &&
      !isLoadingInitial &&
      !workflowAssociationError;

    const handleSubmit = async () => {
      const trimmedName = name.trim();
      if (!trimmedName) {
        setError(t('taskGroupFormDialog.errors.nameRequired'));
        return;
      }

      if (workflowAssociationError) {
        setError(workflowAssociationError);
        return;
      }

      setError(null);

      try {
        const savedGroup =
          props.mode === 'create'
            ? await createTaskGroup.mutateAsync({
                name: trimmedName,
                description: description.trim() || null,
                base_branch: baseBranch,
              })
            : await updateTaskGroup.mutateAsync({
                groupId: props.group.id,
                data: {
                  name: trimmedName,
                  description: description.trim() || null,
                  base_branch: baseBranch,
                },
              });

        const workflowPayload =
          workflowAssociationFormToPayload(workflowAssociation);
        const existingWorkflowAssociation = getAssociationAtScope(
          workflowResolution,
          'task_group_default'
        );

        if (workflowPayload) {
          await taskGroupsApi.upsertWorkflowAssociation(
            savedGroup.id,
            workflowPayload
          );
        } else if (existingWorkflowAssociation) {
          await taskGroupsApi.deleteWorkflowAssociation(savedGroup.id);
        }

        await queryClient.invalidateQueries({
          queryKey: workflowAssociationKeys.taskGroup(savedGroup.id),
        });

        modal.resolve('saved' as SaveResult);
        modal.hide();
      } catch {
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

            <div className="space-y-2">
              <Label htmlFor="group-description">
                {t('taskGroupFormDialog.descriptionLabel')}
              </Label>
              <Textarea
                id="group-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t('taskGroupFormDialog.descriptionPlaceholder')}
                rows={3}
              />
              <p className="text-xs text-muted-foreground">
                {t('taskGroupFormDialog.descriptionHint')}
              </p>
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

            <WorkflowAssociationFields
              title="Task group workflow default"
              description="Set the default workflow metadata for tasks in this group. Task-level overrides take precedence."
              value={workflowAssociation}
              onChange={(updates) =>
                setWorkflowAssociation((previous) => ({
                  ...previous,
                  ...updates,
                }))
              }
              error={workflowAssociationError}
              onClear={() =>
                setWorkflowAssociation(emptyWorkflowAssociationFormState)
              }
            />

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
