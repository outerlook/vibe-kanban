import { useCallback, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { PanelGroup, Panel, PanelResizeHandle } from 'react-resizable-panels';
import { useQueryClient } from '@tanstack/react-query';
import { GitPullRequest } from 'lucide-react';
import { BranchSection, BranchSectionSkeleton } from './index';
import { PrDetailPanel } from './PrDetailPanel';
import {
  buildPrPanelData,
  createEmptyTaskCounts,
} from './prPanelData';
import {
  PushBranchDialog,
  type PushBranchDialogResult,
} from '@/components/dialogs/git/PushBranchDialog';
import { ForcePushBranchDialog } from '@/components/dialogs/git/ForcePushBranchDialog';
import { useNavigateWithSearch } from '@/hooks/useNavigateWithSearch';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { useBatchBranchSyncStatus, branchSyncStatusKeys } from '@/hooks';
import { paths } from '@/lib/paths';
import { cn } from '@/lib/utils';
import type { ProjectPrsResponse } from '@/lib/api';
import type { TaskGroupWithStats, Workspace } from 'shared/types';

type SplitSizes = [number, number];

const MIN_PANEL_SIZE = 20;
const DEFAULT_LIST_DETAIL: SplitSizes = [40, 60];
const STORAGE_KEY = 'prLayout.desktop.v1.listDetail';

function loadSizes(key: string, fallback: SplitSizes): SplitSizes {
  try {
    const saved = localStorage.getItem(key);
    if (!saved) return fallback;
    const parsed = JSON.parse(saved);
    if (Array.isArray(parsed) && parsed.length === 2)
      return parsed as SplitSizes;
    return fallback;
  } catch {
    return fallback;
  }
}

function saveSizes(key: string, sizes: SplitSizes): void {
  try {
    localStorage.setItem(key, JSON.stringify(sizes));
  } catch {
    // Ignore errors
  }
}

export interface PrPanelProps {
  projectId: string;
  prsResponse: ProjectPrsResponse | undefined;
  taskGroups: TaskGroupWithStats[] | undefined;
  workspaces: Workspace[] | undefined;
  hasActiveFilters: boolean;
  isMobile?: boolean;
}

export function PrPanel({
  projectId,
  prsResponse,
  taskGroups,
  workspaces,
  hasActiveFilters,
  isMobile: isMobileProp,
}: PrPanelProps) {
  const { t } = useTranslation(['prs', 'common']);
  const navigate = useNavigateWithSearch();
  const queryClient = useQueryClient();
  const [searchParams] = useSearchParams();
  const isDesktop = useMediaQuery('(min-width: 1024px)');
  const isMobile = isMobileProp ?? !isDesktop;
  const [isListCollapsed, setIsListCollapsed] = useState(false);
  const [panelSizes] = useState<SplitSizes>(() =>
    loadSizes(STORAGE_KEY, DEFAULT_LIST_DETAIL)
  );
  const [pushingBranch, setPushingBranch] = useState<string | null>(null);

  const selectedRepoId = searchParams.get('repo');
  const selectedPrNumber = searchParams.get('pr');
  const hasSelection = selectedRepoId !== null && selectedPrNumber !== null;

  const { groupedByBranch, branchMetadata, selectedPrData } = useMemo(
    () =>
      buildPrPanelData({
        prsResponse,
        taskGroups,
        workspaces,
        selectedRepoId,
        selectedPrNumber,
      }),
    [prsResponse, taskGroups, workspaces, selectedRepoId, selectedPrNumber]
  );

  const primaryRepoId = prsResponse?.repos?.[0]?.repo_id;
  const branchNames = useMemo(
    () => Array.from(groupedByBranch.keys()),
    [groupedByBranch]
  );

  const { data: syncStatusData } = useBatchBranchSyncStatus(
    primaryRepoId,
    projectId,
    branchNames,
    { enabled: branchNames.length > 0 }
  );

  const handleSelectPr = useCallback(
    (repoId: string, prNumber: number | bigint) => {
      navigate({ search: `?repo=${repoId}&pr=${prNumber}` });
    },
    [navigate]
  );

  const handleBackToList = useCallback(() => {
    navigate(paths.projectPrs(projectId));
  }, [navigate, projectId]);

  const handlePush = useCallback(
    async (branchName: string, repoId: string, commitsAhead?: number) => {
      setPushingBranch(branchName);
      try {
        const result: PushBranchDialogResult = await PushBranchDialog.show({
          repoId,
          branchName,
          commitsAhead,
        });

        if (result === 'force_push_required') {
          await ForcePushBranchDialog.show({
            repoId,
            branchName,
          });
        }

        queryClient.invalidateQueries({
          queryKey: branchSyncStatusKeys.batch(repoId, projectId, branchNames),
        });
      } finally {
        setPushingBranch(null);
      }
    },
    [queryClient, projectId, branchNames]
  );

  const prList = (
    <div className="flex-1 overflow-y-auto p-4 space-y-4">
      {groupedByBranch.size === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
          <GitPullRequest className="h-12 w-12 mb-4 opacity-50" />
          <p className="text-lg font-medium">
            {t('prs:noPrsFound', { defaultValue: 'No pull requests found' })}
          </p>
          <p className="text-sm mt-1">
            {hasActiveFilters
              ? t('prs:tryAdjustingFilters', {
                  defaultValue: 'Try adjusting your filters',
                })
              : t('prs:noOpenPrs', {
                  defaultValue:
                    'No open PRs for the currently loaded branches',
                })}
          </p>
        </div>
      ) : (
        Array.from(groupedByBranch.entries()).map(([branchName, prs]) => {
          const meta = branchMetadata.get(branchName);
          const syncStatus = syncStatusData?.statuses?.[branchName];
          const repoId = meta?.repoId;
          return (
            <BranchSection
              key={branchName}
              branchName={branchName}
              prs={prs.map((pr) => ({
                ...pr,
                onClick: () => {
                  const idStr = pr.id.toString();
                  const lastDash = idStr.lastIndexOf('-');
                  const extractedRepoId = idStr.slice(0, lastDash);
                  const prNumber = idStr.slice(lastDash + 1);
                  handleSelectPr(extractedRepoId, Number(prNumber));
                },
                selected:
                  selectedRepoId !== null &&
                  selectedPrNumber !== null &&
                  pr.id === `${selectedRepoId}-${selectedPrNumber}`,
              }))}
              taskCounts={meta?.taskCounts ?? createEmptyTaskCounts()}
              repoId={repoId}
              projectId={projectId}
              workspaceId={meta?.workspaceId}
              groupName={meta?.groupName}
              groupDescription={meta?.groupDescription}
              syncStatus={syncStatus}
              onPush={
                repoId
                  ? () =>
                      handlePush(
                        branchName,
                        repoId,
                        syncStatus?.remote_ahead ?? undefined
                      )
                  : undefined
              }
              isPushing={pushingBranch === branchName}
            />
          );
        })
      )}
    </div>
  );

  const emptyDetailState = (
    <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground">
      <GitPullRequest className="h-12 w-12 mb-4 opacity-50" />
      <p>
        {t('prs:selectPr', {
          defaultValue: 'Select a pull request to view details',
        })}
      </p>
    </div>
  );

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

  if (!hasSelection) {
    return (
      <div className="flex h-full border rounded-lg overflow-hidden bg-background">
        {prList}
      </div>
    );
  }

  return (
    <div className="flex h-full border rounded-lg overflow-hidden bg-background">
      <PanelGroup
        direction="horizontal"
        className="h-full min-h-0"
        onLayout={(layout) => {
          if (layout.length === 2) {
            saveSizes(STORAGE_KEY, [layout[0], layout[1]]);
          }
        }}
      >
        <Panel
          id="pr-list"
          order={1}
          defaultSize={panelSizes[0]}
          minSize={MIN_PANEL_SIZE}
          collapsible
          collapsedSize={0}
          onCollapse={() => setIsListCollapsed(true)}
          onExpand={() => setIsListCollapsed(false)}
          className="min-w-0 min-h-0 overflow-hidden flex flex-col"
          role="region"
          aria-label="Pull request list"
        >
          {prList}
        </Panel>

        <PanelResizeHandle
          id="handle-list-detail"
          className={cn(
            'relative z-30 bg-border cursor-col-resize group touch-none',
            'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
            'focus-visible:ring-offset-1 focus-visible:ring-offset-background',
            'transition-all',
            isListCollapsed ? 'w-6' : 'w-1'
          )}
          aria-label="Resize panels"
          role="separator"
          aria-orientation="vertical"
        >
          <div className="pointer-events-none absolute inset-y-0 left-1/2 -translate-x-1/2 w-px bg-border" />
          <div className="pointer-events-none absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col items-center gap-1 bg-muted/90 border border-border rounded-full px-1.5 py-3 opacity-70 group-hover:opacity-100 group-focus:opacity-100 transition-opacity shadow-sm">
            <span className="w-1 h-1 rounded-full bg-muted-foreground" />
            <span className="w-1 h-1 rounded-full bg-muted-foreground" />
            <span className="w-1 h-1 rounded-full bg-muted-foreground" />
          </div>
        </PanelResizeHandle>

        <Panel
          id="pr-detail"
          order={2}
          defaultSize={panelSizes[1]}
          minSize={MIN_PANEL_SIZE}
          collapsible={false}
          className="min-w-0 min-h-0 overflow-hidden flex flex-col"
          role="region"
          aria-label="Pull request details"
        >
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
        </Panel>
      </PanelGroup>
    </div>
  );
}

export function PrPanelSkeleton({ isMobile = false }: { isMobile?: boolean }) {
  void isMobile;
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
