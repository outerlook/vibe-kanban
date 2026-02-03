import { useState, useMemo, ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { RefreshCw, GitPullRequest, Settings, FolderGit2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import {
  PrFilters,
  PrFiltersSkeleton,
  PrPanel,
  PrPanelSkeleton,
} from '@/components/prs';
import { useProject } from '@/contexts/ProjectContext';
import { useProjectPrs, prKeys } from '@/hooks/useProjectPrs';
import { useProjectWorkspaces } from '@/hooks/useProjectWorkspaces';
import { useTaskGroupStats } from '@/hooks/useTaskGroupStats';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { useQueryClient } from '@tanstack/react-query';
import { ApiError } from '@/lib/api';

interface PageHeaderProps {
  onRefresh?: () => void;
  refreshLabel?: string;
  disabled?: boolean;
  subtitle?: ReactNode;
}

function PageHeader({
  onRefresh,
  refreshLabel = 'Refresh',
  disabled,
  subtitle,
}: PageHeaderProps) {
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-3">
        <GitPullRequest className="h-6 w-6" />
        <h1 className="text-2xl font-semibold">Pull Requests</h1>
        {subtitle}
      </div>
      {onRefresh && (
        <Button
          variant="outline"
          size="sm"
          onClick={onRefresh}
          disabled={disabled}
        >
          <RefreshCw className="h-4 w-4 mr-2" />
          {refreshLabel}
        </Button>
      )}
    </div>
  );
}

export function PrOverview() {
  const queryClient = useQueryClient();
  const { projectId, isLoading: projectLoading } = useProject();
  const isDesktop = useMediaQuery('(min-width: 1024px)');
  const isMobile = !isDesktop;

  const [selectedBranch, setSelectedBranch] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const {
    data: prsResponse,
    isLoading: prsLoading,
    error: prsError,
    refetch,
  } = useProjectPrs(projectId);

  const { data: taskGroups, isLoading: taskGroupsLoading } =
    useTaskGroupStats(projectId);

  const { data: workspaces, isLoading: workspacesLoading } =
    useProjectWorkspaces(projectId);

  // Progressive loading states - show structure early
  const hasTaskGroups = !taskGroupsLoading && taskGroups !== undefined;
  const hasPrData = !prsLoading && prsResponse !== undefined;
  const hasAllData = hasPrData && !workspacesLoading;

  // Get unique base branches from task groups that have base_branch set
  const baseBranches = useMemo(() => {
    if (!taskGroups) return [];
    return [
      ...new Set(
        taskGroups
          .map((g) => g.base_branch)
          .filter((b): b is string => b !== null)
      ),
    ].sort();
  }, [taskGroups]);

  // Count total PRs for display
  const totalPrCount = useMemo(() => {
    if (!prsResponse?.repos) return 0;
    return prsResponse.repos.reduce(
      (sum, repo) => sum + repo.pull_requests.length,
      0
    );
  }, [prsResponse]);

  // Count filtered PRs
  const filteredPrCount = useMemo(() => {
    if (!prsResponse?.repos) return 0;
    let count = 0;
    for (const repo of prsResponse.repos) {
      for (const pr of repo.pull_requests) {
        if (selectedBranch && pr.base_branch !== selectedBranch) continue;
        if (searchQuery && !pr.title.toLowerCase().includes(searchQuery.toLowerCase())) continue;
        count += 1;
      }
    }
    return count;
  }, [prsResponse, selectedBranch, searchQuery]);

  const handleRefresh = () => {
    queryClient.invalidateQueries({ queryKey: prKeys.byProject(projectId) });
    refetch();
  };

  // Check if error is due to GitHub not configured (400 status)
  const isGitHubNotConfigured =
    prsError instanceof ApiError && prsError.status === 400;

  // Check if there are no task groups with base branches
  const hasNoBaseBranches = !taskGroupsLoading && baseBranches.length === 0;

  // Initial loading state - only show when project context is loading
  if (projectLoading) {
    return (
      <div className="flex flex-col h-full p-6 space-y-6">
        <PageHeader onRefresh={handleRefresh} disabled />
        <PrFiltersSkeleton />
        <div className="flex-1 min-h-0">
          <PrPanelSkeleton isMobile={isMobile} />
        </div>
      </div>
    );
  }

  // GitHub not configured state
  if (isGitHubNotConfigured) {
    return (
      <div className="p-6 space-y-6">
        <PageHeader />
        <Alert>
          <Settings className="h-4 w-4" />
          <AlertTitle>GitHub not configured</AlertTitle>
          <AlertDescription className="mt-2">
            <p className="mb-3">
              To view pull requests, you need to configure a GitHub token.
            </p>
            <Button asChild variant="outline" size="sm">
              <Link to="/settings/github">
                <Settings className="h-4 w-4 mr-2" />
                Go to GitHub Settings
              </Link>
            </Button>
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  // General error state
  if (prsError) {
    return (
      <div className="p-6 space-y-6">
        <PageHeader onRefresh={handleRefresh} refreshLabel="Retry" />
        <Alert variant="destructive">
          <AlertTitle>Failed to load pull requests</AlertTitle>
          <AlertDescription>
            {prsError instanceof Error
              ? prsError.message
              : 'An unexpected error occurred'}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  // No task groups with base branches (only show after task groups load)
  if (hasTaskGroups && hasNoBaseBranches) {
    return (
      <div className="flex flex-col h-full p-6 space-y-6">
        <PageHeader onRefresh={handleRefresh} />
        <Alert>
          <FolderGit2 className="h-4 w-4" />
          <AlertTitle>No task groups with base branches</AlertTitle>
          <AlertDescription className="mt-2">
            <p>
              Pull requests are tracked based on task group base branches.
              Create a task group with a base branch to see related PRs here.
            </p>
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full p-6 space-y-6">
      <PageHeader
        onRefresh={handleRefresh}
        subtitle={
          hasAllData ? (
            <span className="text-sm text-muted-foreground">
              ({filteredPrCount} of {totalPrCount})
            </span>
          ) : undefined
        }
      />

      {/* Filters - show immediately with available branch data */}
      {taskGroupsLoading ? (
        <PrFiltersSkeleton />
      ) : (
        <PrFilters
          branches={baseBranches}
          selectedBranch={selectedBranch}
          searchQuery={searchQuery}
          onBranchChange={setSelectedBranch}
          onSearchChange={setSearchQuery}
        />
      )}

      {/* PR Panel with side-by-side layout */}
      <div className="flex-1 min-h-0">
        {!hasPrData || !projectId ? (
          <PrPanelSkeleton isMobile={isMobile} />
        ) : (
          <PrPanel
            projectId={projectId}
            prsResponse={prsResponse}
            taskGroups={taskGroups}
            workspaces={workspaces}
            filters={{ selectedBranch, searchQuery }}
            isMobile={isMobile}
          />
        )}
      </div>
    </div>
  );
}
