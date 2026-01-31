import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import RepoBranchSelector from '@/components/tasks/RepoBranchSelector';
import { ExecutorProfileSelector } from '@/components/settings';
import {
  useNavigateWithSearch,
  useRepoBranchSelection,
  useProjectRepos,
  useTask,
  useTaskGroup,
} from '@/hooks';
import { useProject } from '@/contexts/ProjectContext';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import { useUserSystem } from '@/components/ConfigProvider';
import { attemptsApi } from '@/lib/api';
import { paths } from '@/lib/paths';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import type { ExecutorProfileId, Workspace } from 'shared/types';

export interface BulkCreateAttemptsDialogProps {
  taskIds: string[];
}

type CreationResult = {
  taskId: string;
  success: boolean;
  attemptId?: string;
  error?: string;
};

const BulkCreateAttemptsDialogImpl =
  NiceModal.create<BulkCreateAttemptsDialogProps>(({ taskIds }) => {
    const modal = useModal();
    const navigate = useNavigateWithSearch();
    const { projectId } = useProject();
    const { t } = useTranslation('tasks');
    const { profiles, config } = useUserSystem();
    const { clearSelection } = useTaskSelection();

    const [userSelectedProfile, setUserSelectedProfile] =
      useState<ExecutorProfileId | null>(null);

    const { data: projectRepos = [], isLoading: isLoadingRepos } =
      useProjectRepos(projectId, { enabled: modal.visible });

    const firstTaskId = taskIds[0];
    const { data: firstTask } = useTask(firstTaskId, {
      enabled: modal.visible && !!firstTaskId,
    });
    const firstTaskGroupId = firstTask?.task_group_id ?? undefined;
    const { data: firstTaskGroup } = useTaskGroup(firstTaskGroupId, {
      enabled: modal.visible && !!firstTaskGroupId,
    });

    const {
      configs: repoBranchConfigs,
      isLoading: isLoadingBranches,
      setRepoBranch,
      getWorkspaceRepoInputs,
      reset: resetBranchSelection,
    } = useRepoBranchSelection({
      repos: projectRepos,
      initialBranch: firstTaskGroup?.base_branch,
      enabled: modal.visible && projectRepos.length > 0,
    });

    useEffect(() => {
      if (!modal.visible) {
        setUserSelectedProfile(null);
        resetBranchSelection();
      }
    }, [modal.visible, resetBranchSelection]);

    const defaultProfile = config?.executor_profile ?? null;
    const effectiveProfile = userSelectedProfile ?? defaultProfile;

    const isLoadingInitial = isLoadingRepos || isLoadingBranches;

    const allBranchesSelected = repoBranchConfigs.every(
      (c) => c.targetBranch !== null
    );

    const canCreate = Boolean(
      effectiveProfile &&
        allBranchesSelected &&
        projectRepos.length > 0 &&
        !isLoadingInitial
    );

    const createAttemptForTask = useCallback(
      async (
        taskId: string,
        profile: ExecutorProfileId,
        repos: ReturnType<typeof getWorkspaceRepoInputs>
      ): Promise<CreationResult> => {
        try {
          const attempt: Workspace = await attemptsApi.create({
            task_id: taskId,
            executor_profile_id: profile,
            repos,
          });
          return { taskId, success: true, attemptId: attempt.id };
        } catch (err) {
          const errorMessage =
            err instanceof Error ? err.message : 'Unknown error';
          return { taskId, success: false, error: errorMessage };
        }
      },
      []
    );

    const handleCreate = useCallback(async () => {
      if (
        !effectiveProfile ||
        !allBranchesSelected ||
        projectRepos.length === 0
      ) {
        return;
      }

      const repos = getWorkspaceRepoInputs();
      const total = taskIds.length;

      // Close dialog immediately
      modal.hide();
      clearSelection();

      // Show progress toast
      const toastId = toast.loading(
        t('bulkCreateAttemptsDialog.progress', { current: 1, total })
      );

      const results: CreationResult[] = [];

      for (let i = 0; i < total; i++) {
        toast.loading(
          t('bulkCreateAttemptsDialog.progress', { current: i + 1, total }),
          { id: toastId }
        );

        const result = await createAttemptForTask(
          taskIds[i],
          effectiveProfile,
          repos
        );
        results.push(result);
      }

      const successCount = results.filter((r) => r.success).length;
      const failedCount = total - successCount;
      const firstSuccess = results.find((r) => r.success);

      if (failedCount === 0) {
        toast.success(
          t('bulkCreateAttemptsDialog.summary', {
            success: successCount,
            total,
            failed: 0,
          }),
          {
            id: toastId,
            action: firstSuccess &&
              projectId && {
                label: t('bulkCreateAttemptsDialog.goToFirst'),
                onClick: () =>
                  navigate(
                    paths.attempt(
                      projectId,
                      firstSuccess.taskId,
                      firstSuccess.attemptId!
                    )
                  ),
              },
          }
        );
      } else {
        toast.error(
          t('bulkCreateAttemptsDialog.summary', {
            success: successCount,
            total,
            failed: failedCount,
          }),
          {
            id: toastId,
            description: t('bulkCreateAttemptsDialog.someFailedHint'),
            action: firstSuccess &&
              projectId && {
                label: t('bulkCreateAttemptsDialog.goToFirst'),
                onClick: () =>
                  navigate(
                    paths.attempt(
                      projectId,
                      firstSuccess.taskId,
                      firstSuccess.attemptId!
                    )
                  ),
              },
          }
        );
      }
    }, [
      effectiveProfile,
      allBranchesSelected,
      projectRepos.length,
      getWorkspaceRepoInputs,
      taskIds,
      createAttemptForTask,
      clearSelection,
      projectId,
      navigate,
      modal,
      t,
    ]);

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        modal.hide();
      }
    };

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle>
              {t('bulkCreateAttemptsDialog.title', { count: taskIds.length })}
            </DialogTitle>
            <DialogDescription>
              {t('bulkCreateAttemptsDialog.description', {
                count: taskIds.length,
              })}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {profiles && (
              <ExecutorProfileSelector
                profiles={profiles}
                selectedProfile={effectiveProfile}
                onProfileSelect={setUserSelectedProfile}
                showLabel={true}
              />
            )}

            <RepoBranchSelector
              configs={repoBranchConfigs}
              onBranchChange={setRepoBranch}
              isLoading={isLoadingBranches}
              className="space-y-2"
            />
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => modal.hide()}>
              {t('common:buttons.cancel')}
            </Button>
            <Button onClick={handleCreate} disabled={!canCreate}>
              {t('bulkCreateAttemptsDialog.start', {
                count: taskIds.length,
              })}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  });

export const BulkCreateAttemptsDialog = defineModal<
  BulkCreateAttemptsDialogProps,
  void
>(BulkCreateAttemptsDialogImpl);
