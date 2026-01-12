import { DiffsPanel } from '@/components/panels/DiffsPanel';
import { useAttemptExecution } from '@/hooks';
import type { RepoBranchStatus, TaskWithAttemptStatus, Workspace } from 'shared/types';

interface DiffsPanelContainerProps {
  attempt: Workspace | null;
  selectedTask: TaskWithAttemptStatus | null;
  branchStatus: RepoBranchStatus[] | null;
}

export function DiffsPanelContainer({
  attempt,
  selectedTask,
  branchStatus,
}: DiffsPanelContainerProps) {
  const { isAttemptRunning } = useAttemptExecution(attempt?.id);

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
