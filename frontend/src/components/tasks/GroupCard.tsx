import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Check,
  AlertTriangle,
  GitBranch,
  Loader2,
  GitMerge,
  Pencil,
  Trash2,
} from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import { getTaskGroupColorClass } from '@/lib/ganttColors';
import { useBranchAncestorStatus, useGroupQueueCount } from '@/hooks';
import { useDeleteTaskGroup } from '@/hooks/useTaskGroups';
import { StatusCountBadge } from './StatusCountBadge';
import {
  TaskGroupFormDialog,
  MergeGroupDialog,
  ConfirmDialog,
} from '@/components/dialogs';
import type { TaskGroupWithStats, TaskStatus } from 'shared/types';

const BORDER_COLORS: Record<string, string> = {
  'group-0': 'border-l-indigo-500',
  'group-1': 'border-l-violet-500',
  'group-2': 'border-l-pink-500',
  'group-3': 'border-l-rose-500',
  'group-4': 'border-l-orange-500',
  'group-5': 'border-l-amber-500',
  'group-6': 'border-l-emerald-500',
  'group-7': 'border-l-teal-500',
  'group-8': 'border-l-cyan-500',
  'group-9': 'border-l-sky-500',
};

interface GroupCardProps {
  group: TaskGroupWithStats;
  repoId: string;
  projectId: string;
  onClick?: () => void;
}

const statusOrder: TaskStatus[] = [
  'inprogress',
  'todo',
  'inreview',
  'done',
  'cancelled',
];

export function GroupCard({
  group,
  repoId,
  projectId,
  onClick,
}: GroupCardProps) {
  const { t } = useTranslation('tasks');
  const { data: branchStatus, isLoading: isBranchLoading } =
    useBranchAncestorStatus(repoId, group.base_branch ?? undefined);
  const { data: queueCount } = useGroupQueueCount(group.id);

  const deleteMutation = useDeleteTaskGroup();

  const [contextMenu, setContextMenu] = useState<{
    open: boolean;
    x: number;
    y: number;
  }>({ open: false, x: 0, y: 0 });

  const counts = group.task_counts;
  const colorClass = getTaskGroupColorClass(group.id);
  const borderColorClass = BORDER_COLORS[colorClass] ?? '';

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ open: true, x: e.clientX, y: e.clientY });
  }, []);

  const closeContextMenu = useCallback(() => {
    setContextMenu({ open: false, x: 0, y: 0 });
  }, []);

  const handleMergeInto = useCallback(() => {
    closeContextMenu();
    MergeGroupDialog.show({ sourceGroup: group, projectId });
  }, [closeContextMenu, group, projectId]);

  const handleEdit = useCallback(() => {
    closeContextMenu();
    TaskGroupFormDialog.show({ mode: 'edit', projectId, group });
  }, [closeContextMenu, group, projectId]);

  const handleDelete = useCallback(async () => {
    closeContextMenu();
    const result = await ConfirmDialog.show({
      title: 'Delete Group',
      message: `Are you sure you want to delete "${group.name}"? Tasks in this group will be unassigned.`,
      confirmText: 'Delete',
      cancelText: 'Cancel',
      variant: 'destructive',
    });
    if (result === 'confirmed') {
      await deleteMutation.mutateAsync({ groupId: group.id, projectId });
    }
  }, [closeContextMenu, deleteMutation, group, projectId]);

  return (
    <>
      <Card
        onClick={onClick}
        onContextMenu={handleContextMenu}
        className={cn(
          'p-4 cursor-pointer transition-colors',
          'hover:bg-accent/50 border border-border rounded-lg',
          'border-l-4',
          borderColorClass,
          onClick && 'hover:shadow-sm'
        )}
      >
        <div className="flex flex-col gap-3">
          <div className="flex items-start justify-between gap-2">
            <h3 className="font-medium text-base text-foreground truncate">
              {group.name}
            </h3>

            {group.base_branch && (
              <div className="flex items-center gap-1.5 shrink-0">
                {isBranchLoading ? (
                  <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                ) : branchStatus?.is_ancestor ? (
                  <TooltipProvider>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <div>
                          <Check className="h-4 w-4 text-emerald-500" />
                        </div>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">
                        Base branch is up to date
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                ) : (
                  <TooltipProvider>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <div>
                          <AlertTriangle className="h-4 w-4 text-amber-500" />
                        </div>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">
                        Base branch needs updating
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                )}
              </div>
            )}
          </div>

          {group.base_branch && (
            <Badge
              variant="outline"
              className="w-fit text-xs font-normal gap-1 px-2 py-0.5"
            >
              <GitBranch className="h-3 w-3" />
              {group.base_branch}
            </Badge>
          )}

          <div className="flex flex-wrap gap-1.5">
            {statusOrder.map((status) => (
              <StatusCountBadge
                key={status}
                status={status}
                count={counts[status]}
              />
            ))}
            {queueCount && Number(queueCount.count) > 0 && (
              <Badge
                variant="secondary"
                className="bg-sky-100 text-sky-700 dark:bg-sky-900/30 dark:text-sky-300"
              >
                {Number(queueCount.count)} in queue
              </Badge>
            )}
          </div>
        </div>
      </Card>

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
          <DropdownMenuItem onClick={handleMergeInto}>
            <GitMerge className="h-4 w-4" />
            {t('groupCard.contextMenu.mergeInto')}
          </DropdownMenuItem>
          <DropdownMenuItem onClick={handleEdit}>
            <Pencil className="h-4 w-4" />
            {t('groupCard.contextMenu.edit')}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onClick={handleDelete}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 className="h-4 w-4" />
            {t('groupCard.contextMenu.delete')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  );
}
