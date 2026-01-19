import type { TaskWithAttemptStatus, Workspace } from 'shared/types';

import { useAttemptExecution } from '@/hooks';
import { useGitOperationsError } from '@/contexts/GitOperationsContext';
import { useBranchStatusContext } from '@/contexts/BranchStatusContext';
import { DiffsPanel } from '@/components/panels/DiffsPanel';

export function GitErrorBanner() {
  const { error: gitError } = useGitOperationsError();

  if (!gitError) return null;

  return (
    <div className="mx-4 mt-4 p-3 border border-destructive rounded">
      <div className="text-destructive text-sm">{gitError}</div>
    </div>
  );
}

export function DiffsPanelContainer({
  attempt,
  selectedTask,
}: {
  attempt: Workspace | null;
  selectedTask: TaskWithAttemptStatus | null;
}) {
  const { isAttemptRunning } = useAttemptExecution(attempt?.id);
  const { branchStatus } = useBranchStatusContext();

  return (
    <DiffsPanel
      key={attempt?.id}
      selectedAttempt={attempt}
      gitOps={
        attempt && selectedTask
          ? {
              task: selectedTask,
              branchStatus: branchStatus ?? null,
              isAttemptRunning,
              selectedBranch: branchStatus?.[0]?.target_branch_name ?? null,
            }
          : undefined
      }
    />
  );
}
