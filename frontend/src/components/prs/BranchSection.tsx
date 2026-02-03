import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ChevronDown,
  ChevronRight,
  Check,
  AlertTriangle,
  Loader2,
  GitBranch,
  Code2,
  GitPullRequestCreateArrow,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from '@/components/ui/dropdown-menu';
import { Badge } from '@/components/ui/badge';
import { PrCard, PrCardSkeleton, type PrData } from './PrCard';
import { StatusCountBadge } from '@/components/tasks/StatusCountBadge';
import { IdeIcon, getIdeName, CUSTOM_EDITOR_PREFIX } from '@/components/ide/IdeIcon';
import { useCustomEditors, useOpenInEditor } from '@/hooks';
import { cn } from '@/lib/utils';
import { EditorType } from 'shared/types';
import type { BranchMergeStatus, TaskStatusCounts, TaskStatus } from 'shared/types';
import { CreatePRFromGroupDialog } from '@/components/dialogs/tasks/CreatePRFromGroupDialog';
import { useQueryClient } from '@tanstack/react-query';
import { prKeys } from '@/hooks/useProjectPrs';

const statusOrder: TaskStatus[] = [
  'inprogress',
  'todo',
  'inreview',
  'done',
  'cancelled',
];

export interface BranchSectionProps {
  branchName: string;
  prs: PrData[];
  taskCounts: TaskStatusCounts;
  repoId?: string;
  projectId?: string;
  workspaceId?: string;
  defaultOpen?: boolean;
  className?: string;
  /** Group name for creating a PR (used as default PR title) */
  groupName?: string;
  /** Group description for creating a PR (used as default PR body) */
  groupDescription?: string | null;
  /** Pre-fetched merge status (from batch API) */
  mergeStatus?: BranchMergeStatus;
  /** Whether merge status is being loaded */
  isMergeStatusLoading?: boolean;
}

export function BranchSection({
  branchName,
  prs,
  taskCounts,
  repoId,
  projectId,
  workspaceId,
  defaultOpen = true,
  className,
  groupName,
  groupDescription,
  mergeStatus,
  isMergeStatusLoading = false,
}: BranchSectionProps) {
  const { t } = useTranslation('prs');
  const queryClient = useQueryClient();
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const [contextMenu, setContextMenu] = useState<{
    open: boolean;
    x: number;
    y: number;
  }>({ open: false, x: 0, y: 0 });

  const { data: customEditors = [] } = useCustomEditors();
  const openInEditor = useOpenInEditor(workspaceId);

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

  const toggleOpen = () => setIsOpen(!isOpen);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ open: true, x: e.clientX, y: e.clientY });
  }, []);

  const closeContextMenu = useCallback(() => {
    setContextMenu({ open: false, x: 0, y: 0 });
  }, []);

  const handleOpenInEditor = useCallback(
    (editorValue: string) => {
      closeContextMenu();
      if (!workspaceId) return;
      openInEditor({ editorType: editorValue as EditorType });
    },
    [closeContextMenu, workspaceId, openInEditor]
  );

  const handleCreatePR = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      if (!repoId || !projectId) return;

      await CreatePRFromGroupDialog.show({
        groupName: groupName ?? branchName,
        groupDescription: groupDescription ?? null,
        branchName,
        repoId,
        projectId,
      });

      queryClient.invalidateQueries({ queryKey: prKeys.byProject(projectId) });
    },
    [repoId, projectId, groupName, groupDescription, branchName, queryClient]
  );

  // Show Create PR button when: no PRs AND branch is not merged AND we have group info
  const showCreatePrButton =
    prs.length === 0 &&
    groupName !== undefined &&
    repoId !== undefined &&
    projectId !== undefined &&
    !mergeStatus?.is_merged;

  // Don't render if no PRs and no group info (nothing to show)
  if (prs.length === 0 && !groupName) {
    return null;
  }

  return (
    <>
      <div className={cn('border border-border rounded-md', className)}>
        {/* Collapsible header */}
        <div
          className="flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-accent/50 rounded-t-md"
          onClick={toggleOpen}
          onContextMenu={handleContextMenu}
        >
          {isOpen ? (
            <ChevronDown className="w-4 h-4 flex-shrink-0" />
          ) : (
            <ChevronRight className="w-4 h-4 flex-shrink-0" />
          )}
          <GitBranch className="w-4 h-4 flex-shrink-0 text-muted-foreground" />
          <span className="truncate font-medium">{branchName}</span>

          {/* Git merge status */}
          {repoId && projectId && (
            <span className="flex items-center ml-1">
              {isMergeStatusLoading ? (
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              ) : mergeStatus?.exists ? (
                mergeStatus.is_merged ? (
                  <Check className="h-4 w-4 text-emerald-500" />
                ) : (
                  <AlertTriangle className="h-4 w-4 text-amber-500" />
                )
              ) : null}
            </span>
          )}

          {/* Task status counts */}
          <span className="flex items-center gap-1 ml-auto">
            {statusOrder.map((status) => (
              <StatusCountBadge
                key={status}
                status={status}
                count={taskCounts[status]}
              />
            ))}
          </span>

          {/* PR count */}
          <Badge variant="outline" className="text-xs ml-2">
            {prs.length} PR{prs.length !== 1 ? 's' : ''}
          </Badge>

          {/* Create PR button */}
          {showCreatePrButton && (
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 ml-1"
              onClick={handleCreatePR}
              title={t('branchSection.createPr', 'Create PR')}
            >
              <GitPullRequestCreateArrow className="h-4 w-4" />
            </Button>
          )}
        </div>

        {/* Collapsible content */}
        {isOpen && (
          <div className="px-3 pb-3 space-y-2">
            {prs.map((pr) => (
              <PrCard key={pr.id} pr={pr} />
            ))}
          </div>
        )}
      </div>

      {/* Context menu */}
      <DropdownMenu
        open={contextMenu.open}
        onOpenChange={(open) => {
          if (!open) closeContextMenu();
        }}
      >
        <DropdownMenuContent
          style={{
            position: 'fixed',
            left: contextMenu.x,
            top: contextMenu.y,
          }}
          onCloseAutoFocus={(e) => e.preventDefault()}
        >
          <DropdownMenuSub>
            <DropdownMenuSubTrigger disabled={!workspaceId}>
              {t('branchSection.openInIde', 'Open in IDE')}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              {editorOptions.map((option) => (
                <DropdownMenuItem
                  key={option.value}
                  onClick={() => handleOpenInEditor(option.value)}
                  disabled={!workspaceId}
                >
                  {option.icon}
                  <span className="ml-2">{option.label}</span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  );
}

interface BranchSectionSkeletonProps {
  /** Optional branch name to display instead of placeholder */
  branchName?: string;
  /** Number of PR card skeletons to show */
  prCount?: number;
  /** Staggered animation delay in ms */
  animationDelay?: number;
}

export function BranchSectionSkeleton({
  branchName,
  prCount = 2,
  animationDelay = 0,
}: BranchSectionSkeletonProps) {
  return (
    <div
      className="border border-border rounded-md animate-pulse"
      style={{ animationDelay: `${animationDelay}ms` }}
    >
      {/* Header - show real branch name if available */}
      <div className="px-3 py-2 flex items-center gap-2">
        <ChevronDown className="w-4 h-4 flex-shrink-0 text-muted-foreground/50" />
        <GitBranch className="w-4 h-4 flex-shrink-0 text-muted-foreground/50" />
        {branchName ? (
          <span className="truncate font-medium text-muted-foreground">
            {branchName}
          </span>
        ) : (
          <div className="h-4 bg-muted rounded w-32" />
        )}
        <div className="w-4 h-4 bg-muted rounded" />
        <div className="flex gap-1 ml-auto">
          <div className="h-5 bg-muted rounded w-8" />
          <div className="h-5 bg-muted rounded w-8" />
          <div className="h-5 bg-muted rounded w-8" />
        </div>
        <div className="h-5 bg-muted rounded w-12" />
      </div>

      {/* Content skeleton */}
      <div className="px-3 pb-3 space-y-2">
        {Array.from({ length: prCount }).map((_, i) => (
          <PrCardSkeleton key={i} />
        ))}
      </div>
    </div>
  );
}
