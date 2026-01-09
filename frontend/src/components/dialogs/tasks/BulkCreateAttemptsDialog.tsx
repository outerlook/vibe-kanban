import { useState, useEffect, useMemo, useCallback } from 'react';
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
import RepoBranchSelector from '@/components/tasks/RepoBranchSelector';
import { ExecutorProfileSelector } from '@/components/settings';
import {
  useNavigateWithSearch,
  useRepoBranchSelection,
  useProjectRepos,
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

type CreationState =
  | { phase: 'idle' }
  | { phase: 'creating'; currentIndex: number; results: CreationResult[] }
  | { phase: 'completed'; results: CreationResult[] };

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
    const [creationState, setCreationState] = useState<CreationState>({
      phase: 'idle',
    });

    const { data: projectRepos = [], isLoading: isLoadingRepos } =
      useProjectRepos(projectId, { enabled: modal.visible });

    const {
      configs: repoBranchConfigs,
      isLoading: isLoadingBranches,
      setRepoBranch,
      getWorkspaceRepoInputs,
      reset: resetBranchSelection,
    } = useRepoBranchSelection({
      repos: projectRepos,
      enabled: modal.visible && projectRepos.length > 0,
    });

    useEffect(() => {
      if (!modal.visible) {
        setUserSelectedProfile(null);
        resetBranchSelection();
        setCreationState({ phase: 'idle' });
      }
    }, [modal.visible, resetBranchSelection]);

    const defaultProfile: ExecutorProfileId | null = useMemo(() => {
      return config?.executor_profile ?? null;
    }, [config?.executor_profile]);

    const effectiveProfile = userSelectedProfile ?? defaultProfile;

    const isLoadingInitial = isLoadingRepos || isLoadingBranches;

    const allBranchesSelected = repoBranchConfigs.every(
      (c) => c.targetBranch !== null
    );

    const isCreating = creationState.phase === 'creating';
    const isCompleted = creationState.phase === 'completed';

    const canCreate = Boolean(
      effectiveProfile &&
        allBranchesSelected &&
        projectRepos.length > 0 &&
        !isCreating &&
        !isLoadingInitial &&
        !isCompleted
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
      const results: CreationResult[] = [];

      setCreationState({ phase: 'creating', currentIndex: 0, results: [] });

      for (let i = 0; i < taskIds.length; i++) {
        setCreationState({
          phase: 'creating',
          currentIndex: i,
          results: [...results],
        });

        const result = await createAttemptForTask(
          taskIds[i],
          effectiveProfile,
          repos
        );
        results.push(result);
      }

      setCreationState({ phase: 'completed', results });
    }, [
      effectiveProfile,
      allBranchesSelected,
      projectRepos.length,
      getWorkspaceRepoInputs,
      taskIds,
      createAttemptForTask,
    ]);

    const handleComplete = useCallback(() => {
      if (creationState.phase !== 'completed') return;

      const firstSuccess = creationState.results.find((r) => r.success);
      clearSelection();

      if (firstSuccess && projectId) {
        navigate(
          paths.attempt(projectId, firstSuccess.taskId, firstSuccess.attemptId!)
        );
      }

      modal.hide();
    }, [creationState, clearSelection, projectId, navigate, modal]);

    const handleOpenChange = (open: boolean) => {
      if (!open && !isCreating) {
        modal.hide();
      }
    };

    const successCount =
      creationState.phase === 'completed'
        ? creationState.results.filter((r) => r.success).length
        : 0;
    const failedCount =
      creationState.phase === 'completed'
        ? creationState.results.filter((r) => !r.success).length
        : 0;

    const progressPercent =
      creationState.phase === 'creating'
        ? ((creationState.currentIndex + 1) / taskIds.length) * 100
        : 0;

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle>
              {t('bulkCreateAttemptsDialog.title', { count: taskIds.length })}
            </DialogTitle>
            <DialogDescription>
              {isCompleted
                ? t('bulkCreateAttemptsDialog.completedDescription')
                : t('bulkCreateAttemptsDialog.description', {
                    count: taskIds.length,
                  })}
            </DialogDescription>
          </DialogHeader>

          {isCompleted ? (
            <div className="space-y-4 py-4">
              <div className="text-center">
                <p className="text-sm">
                  {t('bulkCreateAttemptsDialog.summary', {
                    success: successCount,
                    total: taskIds.length,
                    failed: failedCount,
                  })}
                </p>
                {failedCount > 0 && (
                  <p className="text-sm text-muted-foreground mt-2">
                    {t('bulkCreateAttemptsDialog.someFailedHint')}
                  </p>
                )}
              </div>
            </div>
          ) : (
            <div className="space-y-4 py-4">
              {isCreating ? (
                <div className="space-y-3">
                  <p className="text-sm text-muted-foreground text-center">
                    {t('bulkCreateAttemptsDialog.progress', {
                      current: creationState.currentIndex + 1,
                      total: taskIds.length,
                    })}
                  </p>
                  <div className="h-2 w-full bg-secondary rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary transition-all duration-300"
                      style={{ width: `${progressPercent}%` }}
                    />
                  </div>
                </div>
              ) : (
                <>
                  {profiles && (
                    <div className="space-y-2">
                      <ExecutorProfileSelector
                        profiles={profiles}
                        selectedProfile={effectiveProfile}
                        onProfileSelect={setUserSelectedProfile}
                        showLabel={true}
                      />
                    </div>
                  )}

                  <RepoBranchSelector
                    configs={repoBranchConfigs}
                    onBranchChange={setRepoBranch}
                    isLoading={isLoadingBranches}
                    className="space-y-2"
                  />
                </>
              )}
            </div>
          )}

          <DialogFooter>
            {isCompleted ? (
              <Button onClick={handleComplete}>
                {successCount > 0
                  ? t('bulkCreateAttemptsDialog.goToFirst')
                  : t('common:buttons.close')}
              </Button>
            ) : (
              <>
                <Button
                  variant="outline"
                  onClick={() => modal.hide()}
                  disabled={isCreating}
                >
                  {t('common:buttons.cancel')}
                </Button>
                <Button onClick={handleCreate} disabled={!canCreate}>
                  {isCreating
                    ? t('bulkCreateAttemptsDialog.creating')
                    : t('bulkCreateAttemptsDialog.start', {
                        count: taskIds.length,
                      })}
                </Button>
              </>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  });

export const BulkCreateAttemptsDialog =
  defineModal<BulkCreateAttemptsDialogProps, void>(
    BulkCreateAttemptsDialogImpl
  );
