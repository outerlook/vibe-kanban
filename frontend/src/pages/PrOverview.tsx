import { useMemo, ReactNode, useState } from 'react';
import { Link } from 'react-router-dom';
import {
  RefreshCw,
  GitPullRequest,
  Settings,
  FolderGit2,
  Loader2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import {
  PrFilters,
  PrFiltersSkeleton,
  PrPanel,
  PrPanelSkeleton,
} from '@/components/prs';
import { useProject } from '@/contexts/ProjectContext';
import { useProjectPrPages, prKeys } from '@/hooks/useProjectPrPages';
import { useDebouncedCallback } from '@/hooks/useDebouncedCallback';
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
  const [searchInput, setSearchInput] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const { debounced: updateSearchQuery, cancel: cancelSearchQuery } =
    useDebouncedCallback((value: string) => {
      setSearchQuery(value.trim());
    }, 250);

  const {
    data: prsResponse,
    isLoading: prsLoading,
    isFetching: prsFetching,
    isLoadingMore,
    hasMore,
    loadMore,
    loadedCount,
    error: prsError,
    refetch,
  } = useProjectPrPages(projectId, {
    baseBranch: selectedBranch,
    search: searchQuery,
  });

  const { data: taskGroups, isLoading: taskGroupsLoading } =
    useTaskGroupStats(projectId);

  const { data: workspaces, isLoading: workspacesLoading } =
    useProjectWorkspaces(projectId);

  const hasTaskGroups = !taskGroupsLoading && taskGroups !== undefined;
  const hasPrData = !prsLoading && prsResponse !== undefined;
  const hasAllData = hasPrData && !workspacesLoading;

  const baseBranches = useMemo(() => {
    if (!taskGroups) return [];
    return [
      ...new Set(
        taskGroups
          .map((group) => group.base_branch)
          .filter((branch): branch is string => branch !== null)
      ),
    ].sort();
  }, [taskGroups]);

  const handleRefresh = () => {
    cancelSearchQuery();
    queryClient.invalidateQueries({ queryKey: prKeys.project(projectId) });
    refetch();
  };

  const handleSearchChange = (value: string) => {
    setSearchInput(value);

    if (value.trim().length === 0) {
      cancelSearchQuery();
      setSearchQuery('');
      return;
    }

    updateSearchQuery(value);
  };

  const summaryLabel = useMemo(() => {
    if (!hasAllData) {
      return undefined;
    }

    if (hasMore) {
      return `${loadedCount} loaded so far`;
    }

    return `${loadedCount} total matching`;
  }, [hasAllData, hasMore, loadedCount]);

  const isGitHubNotConfigured =
    prsError instanceof ApiError && prsError.status === 400;

  const hasNoBaseBranches = !taskGroupsLoading && baseBranches.length === 0;

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

  if (hasTaskGroups && hasNoBaseBranches) {
    return (
      <div className="flex flex-col h-full p-6 space-y-6">
        <PageHeader onRefresh={handleRefresh} />
        <Alert>
          <FolderGit2 className="h-4 w-4" />
          <AlertTitle>No task groups with base branches</AlertTitle>
          <AlertDescription className="mt-2">
            <p>
              Pull requests are tracked based on task group base branches. Create
              a task group with a base branch to see related PRs here.
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
        disabled={prsFetching}
        subtitle={
          summaryLabel ? (
            <span className="text-sm text-muted-foreground">
              ({summaryLabel})
            </span>
          ) : undefined
        }
      />

      {taskGroupsLoading ? (
        <PrFiltersSkeleton />
      ) : (
        <PrFilters
          branches={baseBranches}
          selectedBranch={selectedBranch}
          searchQuery={searchInput}
          onBranchChange={setSelectedBranch}
          onSearchChange={handleSearchChange}
        />
      )}

      <div className="flex-1 min-h-0 flex flex-col gap-4">
        {!hasPrData || !projectId ? (
          <PrPanelSkeleton isMobile={isMobile} />
        ) : (
          <>
            <div className="flex-1 min-h-0">
              <PrPanel
                projectId={projectId}
                prsResponse={prsResponse}
                taskGroups={taskGroups}
                workspaces={workspaces}
                hasActiveFilters={Boolean(selectedBranch || searchQuery)}
                isMobile={isMobile}
              />
            </div>

            {(loadedCount > 0 || hasMore) && (
              <div className="flex flex-col items-center gap-2 border rounded-lg py-4 px-4 bg-background">
                {hasMore && (
                  <Button
                    onClick={loadMore}
                    disabled={isLoadingMore}
                    variant="secondary"
                  >
                    {isLoadingMore && (
                      <Loader2 className="h-4 w-4 animate-spin mr-2" />
                    )}
                    Load more
                  </Button>
                )}
                <div className="text-xs text-muted-foreground">
                  {hasMore
                    ? `Loaded ${loadedCount} matching pull requests so far`
                    : `Showing all ${loadedCount} matching pull requests`}
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
