import { useState, useMemo } from 'react';
import { Link } from 'react-router-dom';
import { RefreshCw, GitPullRequest, Settings, FolderGit2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import {
  PrFilters,
  PrFiltersSkeleton,
  RepoSection,
  RepoSectionSkeleton,
  type PrData,
} from '@/components/prs';
import { useProject } from '@/contexts/ProjectContext';
import { useProjectPrs, prKeys } from '@/hooks/useProjectPrs';
import { useProjectRepos } from '@/hooks/useProjectRepos';
import { useTaskGroups } from '@/hooks/useTaskGroups';
import { useQueryClient } from '@tanstack/react-query';
import type { ProjectPr } from '@/lib/api';

function mapProjectPrToPrData(pr: ProjectPr): PrData {
  return {
    id: `${pr.repo_id}-${pr.number}`,
    title: pr.title,
    url: pr.url,
    author: pr.author,
    baseBranch: pr.base_branch,
    headBranch: pr.head_branch,
    unresolvedComments: pr.unresolved_comment_count,
    createdAt: pr.created_at,
  };
}

export function PrOverview() {
  const queryClient = useQueryClient();
  const { projectId, isLoading: projectLoading } = useProject();

  const [selectedBranch, setSelectedBranch] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const {
    data: prsResponse,
    isLoading: prsLoading,
    error: prsError,
    refetch,
  } = useProjectPrs(projectId);

  const { data: repos, isLoading: reposLoading } = useProjectRepos(projectId);
  const { data: taskGroups, isLoading: taskGroupsLoading } =
    useTaskGroups(projectId);

  const isLoading = projectLoading || prsLoading || reposLoading || taskGroupsLoading;

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

  // Create repo lookup map
  const reposById = useMemo(() => {
    if (!repos) return new Map<string, string>();
    return new Map(repos.map((r) => [r.id, r.display_name]));
  }, [repos]);

  // Filter and group PRs
  const groupedPrs = useMemo(() => {
    if (!prsResponse?.prs) return new Map<string, PrData[]>();

    const filtered = prsResponse.prs.filter((pr) => {
      // Filter by base branch
      if (selectedBranch && pr.base_branch !== selectedBranch) {
        return false;
      }
      // Filter by search query
      if (
        searchQuery &&
        !pr.title.toLowerCase().includes(searchQuery.toLowerCase())
      ) {
        return false;
      }
      return true;
    });

    // Group by repo_id
    const grouped = new Map<string, PrData[]>();
    for (const pr of filtered) {
      const repoName = reposById.get(pr.repo_id) ?? pr.repo_id;
      const existing = grouped.get(repoName) ?? [];
      existing.push(mapProjectPrToPrData(pr));
      grouped.set(repoName, existing);
    }

    return grouped;
  }, [prsResponse, selectedBranch, searchQuery, reposById]);

  const handleRefresh = () => {
    queryClient.invalidateQueries({ queryKey: prKeys.byProject(projectId) });
    refetch();
  };

  // Check if error is due to GitHub not configured (400 status)
  const isGitHubNotConfigured =
    prsError &&
    'status' in prsError &&
    (prsError as { status?: number }).status === 400;

  // Check if there are no task groups with base branches
  const hasNoBaseBranches = !taskGroupsLoading && baseBranches.length === 0;

  // Loading state
  if (isLoading) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <GitPullRequest className="h-6 w-6" />
            <h1 className="text-2xl font-semibold">Pull Requests</h1>
          </div>
          <Button variant="outline" size="sm" disabled>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
        </div>

        <PrFiltersSkeleton />

        <div className="space-y-4">
          <RepoSectionSkeleton prCount={2} />
          <RepoSectionSkeleton prCount={3} />
        </div>
      </div>
    );
  }

  // GitHub not configured state
  if (isGitHubNotConfigured) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <GitPullRequest className="h-6 w-6" />
            <h1 className="text-2xl font-semibold">Pull Requests</h1>
          </div>
        </div>

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
  if (prsError && !isGitHubNotConfigured) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <GitPullRequest className="h-6 w-6" />
            <h1 className="text-2xl font-semibold">Pull Requests</h1>
          </div>
          <Button variant="outline" size="sm" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Retry
          </Button>
        </div>

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

  // No task groups with base branches
  if (hasNoBaseBranches) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <GitPullRequest className="h-6 w-6" />
            <h1 className="text-2xl font-semibold">Pull Requests</h1>
          </div>
          <Button variant="outline" size="sm" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
        </div>

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

  // No PRs found
  const totalPrs = prsResponse?.prs.length ?? 0;
  const filteredCount = Array.from(groupedPrs.values()).reduce(
    (sum, prs) => sum + prs.length,
    0
  );

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <GitPullRequest className="h-6 w-6" />
          <h1 className="text-2xl font-semibold">Pull Requests</h1>
          <span className="text-sm text-muted-foreground">
            ({filteredCount} of {totalPrs})
          </span>
        </div>
        <Button variant="outline" size="sm" onClick={handleRefresh}>
          <RefreshCw className="h-4 w-4 mr-2" />
          Refresh
        </Button>
      </div>

      {/* Filters */}
      <PrFilters
        branches={baseBranches}
        selectedBranch={selectedBranch}
        searchQuery={searchQuery}
        onBranchChange={setSelectedBranch}
        onSearchChange={setSearchQuery}
      />

      {/* PR List grouped by repo */}
      {groupedPrs.size === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <GitPullRequest className="h-12 w-12 mx-auto mb-4 opacity-50" />
          <p className="text-lg font-medium">No pull requests found</p>
          <p className="text-sm mt-1">
            {selectedBranch || searchQuery
              ? 'Try adjusting your filters'
              : 'No open PRs for the configured base branches'}
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {Array.from(groupedPrs.entries()).map(([repoName, prs]) => (
            <RepoSection key={repoName} repoName={repoName} prs={prs} />
          ))}
        </div>
      )}
    </div>
  );
}
