import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import {
  ArrowLeft,
  ExternalLink,
  MessageSquarePlus,
  Code2,
  GitPullRequest,
  AlertCircle,
  MessageSquare,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { PrThreadItem } from './PrThreadItem';
import type { PrData } from './PrCard';
import { IdeIcon, getIdeName, CUSTOM_EDITOR_PREFIX } from '@/components/ide/IdeIcon';
import { NewConversationDialog } from '@/components/dialogs/conversations/NewConversationDialog';
import { usePrThreads } from '@/hooks/usePrThreads';
import { useProjectWorkspaces } from '@/hooks/useProjectWorkspaces';
import { useCustomEditors, useOpenInEditor } from '@/hooks';
import { paths } from '@/lib/paths';
import { formatShortDate } from '@/lib/utils';
import { EditorType } from 'shared/types';

export interface PrDetailPanelProps {
  projectId: string;
  repoId: string;
  prNumber: number;
  prData: PrData;
  onBack?: () => void;
  isMobile?: boolean;
}

function ThreadSkeleton() {
  return (
    <div className="p-3 bg-muted/50 rounded-md border border-border animate-pulse">
      {/* Header skeleton */}
      <div className="flex items-center justify-between gap-2 mb-2">
        <div className="flex items-center gap-2">
          <div className="w-4 h-4 bg-muted rounded flex-shrink-0" />
          <div className="h-4 bg-muted rounded w-24" />
        </div>
        <div className="h-3 bg-muted rounded w-20" />
      </div>
      {/* Body skeleton */}
      <div className="space-y-2 mt-2">
        <div className="h-3 bg-muted rounded w-full" />
        <div className="h-3 bg-muted rounded w-3/4" />
      </div>
    </div>
  );
}

export function PrDetailPanel({
  projectId,
  repoId,
  prNumber,
  prData,
  onBack,
  isMobile = false,
}: PrDetailPanelProps) {
  const { t } = useTranslation(['prs', 'common']);
  const navigate = useNavigate();

  // Fetch threads
  const {
    data: threadsData,
    isLoading: isThreadsLoading,
    error: threadsError,
  } = usePrThreads(projectId, repoId, prNumber);

  // Fetch workspaces to find one for the PR's head branch
  const { data: workspaces = [] } = useProjectWorkspaces(projectId);

  // Find workspace matching the PR's head branch
  const matchingWorkspace = useMemo(() => {
    return workspaces.find((ws) => ws.branch === prData.headBranch);
  }, [workspaces, prData.headBranch]);

  const { data: customEditors = [] } = useCustomEditors();
  const openInEditor = useOpenInEditor(matchingWorkspace?.id);

  const editorOptions = useMemo(() => {
    const builtIn = Object.values(EditorType)
      .filter((type) => type !== EditorType.CUSTOM)
      .map((editorType) => ({
        value: editorType,
        label: getIdeName(editorType),
        icon: <IdeIcon editorType={editorType} className="h-3.5 w-3.5" />,
        isCustom: false,
      }));

    const custom = customEditors.map((editor) => ({
      value: `${CUSTOM_EDITOR_PREFIX}${editor.id}`,
      label: editor.name,
      icon: <Code2 className="h-3.5 w-3.5" />,
      isCustom: true,
    }));

    return [...builtIn, ...custom];
  }, [customEditors]);

  const handleNewConversation = async () => {
    const result = await NewConversationDialog.show({
      projectId,
      defaultBaseBranch: prData.headBranch,
    });

    if (result) {
      navigate(paths.conversation(projectId, result.id));
    }
  };

  const handleOpenInEditor = (editorValue: string) => {
    if (!matchingWorkspace) return;
    openInEditor({ editorType: editorValue as EditorType });
  };

  const threads = threadsData?.threads ?? [];
  const hasThreads = threads.length > 0;

  // Check if error is GitHub auth related
  const isGithubAuthError =
    threadsError?.message?.includes('github') ||
    threadsError?.message?.includes('auth');

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-start gap-3 p-4 border-b">
        {isMobile && onBack && (
          <Button
            variant="ghost"
            size="icon"
            onClick={onBack}
            className="flex-shrink-0 -ml-2"
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
        )}

        <div className="flex-1 min-w-0">
          <div className="flex items-start justify-between gap-2">
            <div className="flex items-start gap-2 min-w-0 flex-1">
              <GitPullRequest className="w-5 h-5 text-muted-foreground flex-shrink-0 mt-0.5" />
              <h2 className="text-lg font-semibold truncate" title={prData.title}>
                {prData.title}
              </h2>
            </div>
            <a
              href={prData.url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-muted-foreground hover:text-foreground transition-colors flex-shrink-0"
              aria-label={t('prs:openInGithub', { defaultValue: 'Open in GitHub' })}
            >
              <ExternalLink className="w-4 h-4" />
            </a>
          </div>

          {/* Metadata */}
          <div className="mt-2 flex items-center gap-3 text-sm text-muted-foreground flex-wrap">
            <span>@{prData.author}</span>
            <span className="font-mono text-xs">
              {prData.baseBranch}
              <span className="mx-1">&larr;</span>
              {prData.headBranch}
            </span>
            <span>{formatShortDate(prData.createdAt)}</span>
          </div>
        </div>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2 p-4 border-b">
        <Button
          variant="outline"
          size="sm"
          onClick={handleNewConversation}
          className="gap-1.5"
        >
          <MessageSquarePlus className="h-4 w-4" />
          {t('prs:newConversation', { defaultValue: 'New Conversation' })}
        </Button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="outline"
              size="sm"
              disabled={!matchingWorkspace}
              className="gap-1.5"
            >
              <Code2 className="h-4 w-4" />
              {t('prs:openInIde', { defaultValue: 'Open in IDE' })}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {editorOptions.map((option) => (
              <DropdownMenuItem
                key={option.value}
                onClick={() => handleOpenInEditor(option.value)}
                className="gap-2"
              >
                {option.icon}
                <span>{option.label}</span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* Threads Section */}
      <div className="flex-1 overflow-y-auto p-4">
        <h3 className="text-sm font-medium text-muted-foreground mb-3 flex items-center gap-2">
          <MessageSquare className="h-4 w-4" />
          {t('prs:reviewThreads', { defaultValue: 'Review Threads' })}
        </h3>

        {/* Error State */}
        {threadsError && (
          <Alert variant="destructive" className="mb-4">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              {isGithubAuthError
                ? t('prs:errors.githubAuthFailed', {
                    defaultValue:
                      'Failed to fetch review threads. Please check your GitHub authentication.',
                  })
                : t('prs:errors.fetchThreadsFailed', {
                    defaultValue: 'Failed to load review threads.',
                  })}
            </AlertDescription>
          </Alert>
        )}

        {/* Loading State */}
        {isThreadsLoading && (
          <div className="space-y-3">
            <ThreadSkeleton />
            <ThreadSkeleton />
            <ThreadSkeleton />
          </div>
        )}

        {/* Empty State */}
        {!isThreadsLoading && !threadsError && !hasThreads && (
          <div className="flex flex-col items-center justify-center py-8 text-center">
            <MessageSquare className="h-8 w-8 text-muted-foreground/50 mb-2" />
            <p className="text-sm text-muted-foreground">
              {t('prs:noThreads', {
                defaultValue: 'No review threads on this pull request.',
              })}
            </p>
          </div>
        )}

        {/* Threads List */}
        {!isThreadsLoading && !threadsError && hasThreads && (
          <div className="space-y-3">
            {threads.map((thread) => (
              <PrThreadItem key={thread.id} thread={thread} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export function PrDetailPanelSkeleton({ isMobile = false }: { isMobile?: boolean }) {
  return (
    <div className="flex flex-col h-full animate-pulse">
      {/* Header skeleton */}
      <div className="flex items-start gap-3 p-4 border-b">
        {isMobile && (
          <div className="w-8 h-8 bg-muted rounded flex-shrink-0" />
        )}
        <div className="flex-1">
          <div className="flex items-start gap-2">
            <div className="w-5 h-5 bg-muted rounded flex-shrink-0" />
            <div className="h-6 bg-muted rounded w-3/4" />
          </div>
          <div className="mt-2 flex items-center gap-3">
            <div className="h-4 bg-muted rounded w-20" />
            <div className="h-4 bg-muted rounded w-32" />
            <div className="h-4 bg-muted rounded w-24" />
          </div>
        </div>
      </div>

      {/* Actions skeleton */}
      <div className="flex items-center gap-2 p-4 border-b">
        <div className="h-8 bg-muted rounded w-36" />
        <div className="h-8 bg-muted rounded w-28" />
      </div>

      {/* Threads skeleton */}
      <div className="flex-1 p-4">
        <div className="h-4 bg-muted rounded w-32 mb-3" />
        <div className="space-y-3">
          <ThreadSkeleton />
          <ThreadSkeleton />
        </div>
      </div>
    </div>
  );
}
