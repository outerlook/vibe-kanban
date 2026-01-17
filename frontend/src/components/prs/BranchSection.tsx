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
import { IdeIcon, getIdeName } from '@/components/ide/IdeIcon';
import { useBranchAncestorStatus, useCustomEditors, useOpenInEditor } from '@/hooks';
import { cn } from '@/lib/utils';
import { EditorType } from 'shared/types';
import type { TaskStatusCounts, TaskStatus } from 'shared/types';

const CUSTOM_EDITOR_PREFIX = 'custom:';

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
  workspaceId?: string;
  projectId: string;
  defaultOpen?: boolean;
  className?: string;
}

export function BranchSection({
  branchName,
  prs,
  taskCounts,
  repoId,
  workspaceId,
  defaultOpen = true,
  className,
}: BranchSectionProps) {
  const { t } = useTranslation('prs');
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const [contextMenu, setContextMenu] = useState<{
    open: boolean;
    x: number;
    y: number;
  }>({ open: false, x: 0, y: 0 });

  const { data: branchStatus, isLoading: isBranchLoading } =
    useBranchAncestorStatus(repoId, branchName);

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

  if (prs.length === 0) {
    return null;
  }

  return (
    <>
      <div className={cn('border border-border rounded-md', className)}>
        {/* Collapsible header */}
        <Button
          variant="ghost"
          className="w-full justify-start gap-2 px-3 py-2 h-auto font-medium"
          onClick={toggleOpen}
          onContextMenu={handleContextMenu}
        >
          {isOpen ? (
            <ChevronDown className="w-4 h-4 flex-shrink-0" />
          ) : (
            <ChevronRight className="w-4 h-4 flex-shrink-0" />
          )}
          <GitBranch className="w-4 h-4 flex-shrink-0 text-muted-foreground" />
          <span className="truncate">{branchName}</span>

          {/* Git sync status */}
          {repoId && (
            <span className="flex items-center ml-1">
              {isBranchLoading ? (
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              ) : branchStatus?.is_ancestor ? (
                <Check className="h-4 w-4 text-emerald-500" />
              ) : (
                <AlertTriangle className="h-4 w-4 text-amber-500" />
              )}
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
        </Button>

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

export function BranchSectionSkeleton({ prCount = 2 }: { prCount?: number }) {
  return (
    <div className="border border-border rounded-md animate-pulse">
      {/* Header skeleton */}
      <div className="px-3 py-2 flex items-center gap-2">
        <div className="w-4 h-4 bg-muted rounded" />
        <div className="w-4 h-4 bg-muted rounded" />
        <div className="h-4 bg-muted rounded w-32" />
        <div className="w-4 h-4 bg-muted rounded" />
        <div className="flex gap-1 ml-auto">
          <div className="h-5 bg-muted rounded w-12" />
          <div className="h-5 bg-muted rounded w-10" />
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
