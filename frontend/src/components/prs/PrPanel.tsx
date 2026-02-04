import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { GitPullRequest } from 'lucide-react';
import { BranchSection, BranchSectionSkeleton, type PrData } from './index';
import { PrDetailPanel } from './PrDetailPanel';
import { useNavigateWithSearch } from '@/hooks/useNavigateWithSearch';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { paths } from '@/lib/paths';
import type { ProjectPrsResponse, PrWithComments } from '@/lib/api';
import type { TaskGroupWithStats, Workspace, TaskStatusCounts } from 'shared/types';

function toPrData(pr: PrWithComments, repoId: string): PrData {
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

export interface PrPanelFilters {
  selectedBranch: string | null;
  searchQuery: string;
}

export interface PrPanelProps {
  projectId: string;
  prsResponse: ProjectPrsResponse | undefined;
  taskGroups: TaskGroupWithStats[] | undefined;
  workspaces: Workspace[] | undefined;
  filters: PrPanelFilters;
  isMobile?: boolean;
}

type BranchMetadata = {
  taskCounts: TaskStatusCounts;
  repoId?: string;
  workspaceId?: string;
  groupName?: string;
  groupDescription?: string | null;
};

export function PrPanel({
  projectId,
  prsResponse,
  taskGroups,
  workspaces,
  filters,
  isMobile: isMobileProp,
}: PrPanelProps) {
  const { t } = useTranslation(['prs', 'common']);
  const navigate = useNavigateWithSearch();
  const [searchParams] = useSearchParams();
  const isDesktop = useMediaQuery('(min-width: 1024px)');
  const isMobile = isMobileProp ?? !isDesktop;

  // Read selection from URL query params
  const selectedRepoId = searchParams.get('repo');
  const selectedPrNumber = searchParams.get('pr');
  const hasSelection = selectedRepoId !== null && selectedPrNumber !== null;

  // Group PRs by head branch and build metadata
  const { groupedByBranch, branchMetadata, selectedPrData } = useMemo(() => {
    const grouped = new Map<string, PrData[]>();
    const metadata = new Map<string, BranchMetadata>();
    let foundPrData: PrData | undefined;

    // First, collect all branches from task groups that have base_branch
    if (taskGroups) {
      for (const group of taskGroups) {
        if (!group.base_branch) continue;

        const branchName = group.base_branch;
        if (metadata.has(branchName)) continue;

        const workspace = workspaces?.find((w) => w.branch === branchName);
        const repoId = prsResponse?.repos?.[0]?.repo_id;

        metadata.set(branchName, {
          taskCounts: { ...group.task_counts },
          repoId,
          workspaceId: workspace?.id,
          groupName: group.name,
          groupDescription: group.description,
        });

        grouped.set(branchName, []);
      }
    }

    // Then, process PRs
    if (prsResponse?.repos) {
      for (const repo of prsResponse.repos) {
        for (const pr of repo.pull_requests) {
          // Filter by base branch
          if (filters.selectedBranch && pr.base_branch !== filters.selectedBranch) {
            continue;
          }
          // Filter by search query
          if (
            filters.searchQuery &&
            !pr.title.toLowerCase().includes(filters.searchQuery.toLowerCase())
          ) {
            continue;
          }

          const branchName = pr.head_branch;
          const prData = toPrData(pr, repo.repo_id);

          // Check if this is the selected PR
          if (
            selectedRepoId === repo.repo_id &&
            selectedPrNumber === String(pr.number)
          ) {
            foundPrData = prData;
          }

          // Group PRs by head branch
          const existing = grouped.get(branchName) ?? [];
          existing.push(prData);
          grouped.set(branchName, existing);

          // Update or initialize metadata
          if (!metadata.has(branchName)) {
            const matchingGroups =
              taskGroups?.filter((g) => g.base_branch === branchName) ?? [];
            const aggregatedCounts: TaskStatusCounts = {
              todo: BigInt(0),
              inprogress: BigInt(0),
              inreview: BigInt(0),
              done: BigInt(0),
              cancelled: BigInt(0),
            };
            for (const group of matchingGroups) {
              aggregatedCounts.todo += group.task_counts.todo;
              aggregatedCounts.inprogress += group.task_counts.inprogress;
              aggregatedCounts.inreview += group.task_counts.inreview;
              aggregatedCounts.done += group.task_counts.done;
              aggregatedCounts.cancelled += group.task_counts.cancelled;
            }

            const workspace = workspaces?.find((w) => w.branch === branchName);
            const firstGroup = matchingGroups[0];

            metadata.set(branchName, {
              taskCounts: aggregatedCounts,
              repoId: repo.repo_id,
              workspaceId: workspace?.id,
              groupName: firstGroup?.name,
              groupDescription: firstGroup?.description,
            });
          } else {
            const existingMeta = metadata.get(branchName)!;
            if (!existingMeta.repoId) {
              existingMeta.repoId = repo.repo_id;
            }
          }
        }
      }
    }

    return { groupedByBranch: grouped, branchMetadata: metadata, selectedPrData: foundPrData };
  }, [prsResponse, taskGroups, workspaces, filters, selectedRepoId, selectedPrNumber]);

  const handleSelectPr = useCallback(
    (repoId: string, prNumber: number | bigint) => {
      navigate({ search: `?repo=${repoId}&pr=${prNumber}` });
    },
    [navigate]
  );

  const handleBackToList = useCallback(() => {
    navigate(paths.projectPrs(projectId));
  }, [navigate, projectId]);

  // Render the PR list with BranchSection components
  const prList = (
    <div className="flex-1 overflow-y-auto p-4 space-y-4">
      {groupedByBranch.size === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
          <GitPullRequest className="h-12 w-12 mb-4 opacity-50" />
          <p className="text-lg font-medium">
            {t('prs:noPrsFound', { defaultValue: 'No pull requests found' })}
          </p>
          <p className="text-sm mt-1">
            {filters.selectedBranch || filters.searchQuery
              ? t('prs:tryAdjustingFilters', { defaultValue: 'Try adjusting your filters' })
              : t('prs:noOpenPrs', { defaultValue: 'No open PRs for the configured base branches' })}
          </p>
        </div>
      ) : (
        Array.from(groupedByBranch.entries()).map(([branchName, prs]) => {
          const meta = branchMetadata.get(branchName);
          return (
            <BranchSection
              key={branchName}
              branchName={branchName}
              prs={prs.map((pr) => ({
                ...pr,
                onClick: () => {
                  // Extract repoId and prNumber from pr.id which is `${repoId}-${prNumber}`
                  const [repoId, ...prNumberParts] = pr.id.toString().split('-');
                  const prNumber = prNumberParts.join('-');
                  handleSelectPr(repoId, Number(prNumber));
                },
                selected:
                  selectedRepoId !== null &&
                  selectedPrNumber !== null &&
                  pr.id === `${selectedRepoId}-${selectedPrNumber}`,
              }))}
              taskCounts={
                meta?.taskCounts ?? {
                  todo: BigInt(0),
                  inprogress: BigInt(0),
                  inreview: BigInt(0),
                  done: BigInt(0),
                  cancelled: BigInt(0),
                }
              }
              repoId={meta?.repoId}
              projectId={projectId}
              workspaceId={meta?.workspaceId}
              groupName={meta?.groupName}
              groupDescription={meta?.groupDescription}
            />
          );
        })
      )}
    </div>
  );

  // Empty state for detail panel
  const emptyDetailState = (
    <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground">
      <GitPullRequest className="h-12 w-12 mb-4 opacity-50" />
      <p>
        {t('prs:selectPr', { defaultValue: 'Select a pull request to view details' })}
      </p>
    </div>
  );

  // Mobile layout: show either list or detail (full-screen)
  if (isMobile) {
    if (hasSelection && selectedPrData && selectedRepoId) {
      return (
        <div className="flex h-full flex-col border rounded-lg overflow-hidden bg-background">
          <PrDetailPanel
            projectId={projectId}
            repoId={selectedRepoId}
            prNumber={Number(selectedPrNumber)}
            prData={selectedPrData}
            onBack={handleBackToList}
            isMobile
          />
        </div>
      );
    }

    return (
      <div className="flex h-full flex-col border rounded-lg overflow-hidden bg-background">
        {prList}
      </div>
    );
  }

  // Desktop layout: click-to-open pattern
  // Show only list by default, side-by-side when a PR is selected
  if (!hasSelection) {
    return (
      <div className="flex h-full border rounded-lg overflow-hidden bg-background">
        {prList}
      </div>
    );
  }

  // Desktop with selection: side-by-side layout
  return (
    <div className="flex h-full border rounded-lg overflow-hidden bg-background">
      {/* Sidebar with PR list */}
      <div className="w-80 border-r flex-shrink-0 flex flex-col">
        {prList}
      </div>

      {/* Main content area - detail panel */}
      <div className="flex-1 flex flex-col min-w-0">
        {selectedPrData && selectedRepoId ? (
          <PrDetailPanel
            projectId={projectId}
            repoId={selectedRepoId}
            prNumber={Number(selectedPrNumber)}
            prData={selectedPrData}
          />
        ) : (
          emptyDetailState
        )}
      </div>
    </div>
  );
}

export function PrPanelSkeleton({ isMobile = false }: { isMobile?: boolean }) {
  // Both mobile and desktop show full-width list skeleton (click-to-open pattern)
  void isMobile; // unused but kept for API consistency
  return (
    <div className="flex h-full flex-col border rounded-lg overflow-hidden bg-background">
      <div className="p-4 space-y-4">
        <BranchSectionSkeleton animationDelay={0} />
        <BranchSectionSkeleton animationDelay={100} />
      </div>
    </div>
  );
}

export default PrPanel;
