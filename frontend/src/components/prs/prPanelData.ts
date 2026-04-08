import type { ProjectPrsResponse, PrWithComments } from '@/lib/api';
import type {
  TaskGroupWithStats,
  TaskStatusCounts,
  Workspace,
} from 'shared/types';
import type { PrData } from './PrCard';

export type BranchMetadata = {
  taskCounts: TaskStatusCounts;
  repoId?: string;
  workspaceId?: string;
  groupName?: string;
  groupDescription?: string | null;
};

export function createEmptyTaskCounts(): TaskStatusCounts {
  return {
    todo: BigInt(0),
    inprogress: BigInt(0),
    inreview: BigInt(0),
    done: BigInt(0),
    cancelled: BigInt(0),
  };
}

export function toPrData(pr: PrWithComments, repoId: string): PrData {
  return {
    id: `${repoId}-${pr.number}`,
    title: pr.title,
    url: pr.url,
    author: pr.author,
    baseBranch: pr.base_branch,
    headBranch: pr.head_branch,
    unresolvedComments: pr.unresolved_count,
    createdAt: pr.created_at,
  };
}

function sumTaskCounts(groups: TaskGroupWithStats[]): TaskStatusCounts {
  return groups.reduce<TaskStatusCounts>((totals, group) => {
    totals.todo += group.task_counts.todo;
    totals.inprogress += group.task_counts.inprogress;
    totals.inreview += group.task_counts.inreview;
    totals.done += group.task_counts.done;
    totals.cancelled += group.task_counts.cancelled;
    return totals;
  }, createEmptyTaskCounts());
}

export function buildPrPanelData({
  prsResponse,
  taskGroups,
  workspaces,
  selectedRepoId,
  selectedPrNumber,
}: {
  prsResponse: ProjectPrsResponse | undefined;
  taskGroups: TaskGroupWithStats[] | undefined;
  workspaces: Workspace[] | undefined;
  selectedRepoId: string | null;
  selectedPrNumber: string | null;
}): {
  groupedByBranch: Map<string, PrData[]>;
  branchMetadata: Map<string, BranchMetadata>;
  selectedPrData: PrData | undefined;
} {
  const groupedByBranch = new Map<string, PrData[]>();
  const branchMetadata = new Map<string, BranchMetadata>();
  let selectedPrData: PrData | undefined;

  for (const repo of prsResponse?.repos ?? []) {
    for (const pr of repo.pull_requests) {
      const branchName = pr.head_branch;
      const prData = toPrData(pr, repo.repo_id);

      if (
        selectedRepoId === repo.repo_id &&
        selectedPrNumber === String(pr.number)
      ) {
        selectedPrData = prData;
      }

      const branchPrs = groupedByBranch.get(branchName) ?? [];
      branchPrs.push(prData);
      groupedByBranch.set(branchName, branchPrs);

      if (!branchMetadata.has(branchName)) {
        const matchingGroups =
          taskGroups?.filter((group) => group.base_branch === branchName) ?? [];
        const workspace = workspaces?.find((entry) => entry.branch === branchName);
        const firstGroup = matchingGroups[0];

        branchMetadata.set(branchName, {
          taskCounts: sumTaskCounts(matchingGroups),
          repoId: repo.repo_id,
          workspaceId: workspace?.id,
          groupName: firstGroup?.name,
          groupDescription: firstGroup?.description,
        });
      }
    }
  }

  return { groupedByBranch, branchMetadata, selectedPrData };
}
