import { useState, type Dispatch, type SetStateAction } from 'react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronRight, Check, Star } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { defineModal } from '@/lib/modals';
import { cn } from '@/lib/utils';
import { useTaskDependencyTree } from '@/hooks/useTaskDependencies';
import { taskDependenciesApi } from '@/lib/api';
import type { TaskDependencyTreeNode } from '@/lib/api';
import type { Task, TaskStatus } from 'shared/types';
import { statusLabels } from '@/utils/statusLabels';

export interface DependencyTreeDialogProps {
  taskId: string;
  maxDepth?: number;
}

const DEFAULT_MAX_DEPTH = 5;

const statusToneClasses: Record<TaskStatus, { dot: string; text: string }> = {
  todo: { dot: 'bg-slate-400', text: 'text-slate-500' },
  inprogress: { dot: 'bg-blue-500', text: 'text-blue-600' },
  inreview: { dot: 'bg-blue-500', text: 'text-blue-600' },
  done: { dot: 'bg-emerald-500', text: 'text-emerald-600' },
  cancelled: { dot: 'bg-slate-400', text: 'text-slate-500' },
};

const buildBlockingTree = async (
  task: Task,
  maxDepth: number,
  path: Set<string> = new Set()
): Promise<TaskDependencyTreeNode> => {
  if (maxDepth <= 0) {
    return { task, dependencies: [] };
  }
  if (path.has(task.id)) {
    return { task, dependencies: [] };
  }

  const nextPath = new Set(path);
  nextPath.add(task.id);

  const dependencies = await taskDependenciesApi.getDependencies(
    task.id,
    'blocking'
  );

  const nodes = await Promise.all(
    dependencies.map((dependency) =>
      buildBlockingTree(dependency, maxDepth - 1, nextPath)
    )
  );

  return { task, dependencies: nodes };
};

const DependencyTreeDialogImpl = NiceModal.create<DependencyTreeDialogProps>(
  ({ taskId, maxDepth = DEFAULT_MAX_DEPTH }) => {
    const modal = useModal();
    const { t } = useTranslation('tasks');
    const [expandedUpstream, setExpandedUpstream] = useState<Set<string>>(
      () => new Set()
    );
    const [expandedDownstream, setExpandedDownstream] = useState<Set<string>>(
      () => new Set()
    );

    const {
      data: upstreamTree,
      isLoading: isUpstreamLoading,
      isError: isUpstreamError,
      refetch: refetchUpstream,
    } = useTaskDependencyTree(taskId, maxDepth);

    const currentTask = upstreamTree?.task;

    const {
      data: downstreamTree,
      isLoading: isDownstreamLoading,
      isError: isDownstreamError,
      refetch: refetchDownstream,
    } = useQuery<TaskDependencyTreeNode>({
      queryKey: ['taskDependencyTree', taskId, 'blocking', maxDepth],
      queryFn: async () => {
        if (!currentTask) {
          throw new Error('Missing task for dependency tree.');
        }
        return buildBlockingTree(currentTask, maxDepth);
      },
      enabled: Boolean(currentTask),
      staleTime: 10_000,
      retry: 2,
    });

    const downstreamLoading =
      isDownstreamLoading || (isUpstreamLoading && !currentTask);
    const downstreamError =
      isDownstreamError || (isUpstreamError && !currentTask);
    const retryDownstream = currentTask ? refetchDownstream : refetchUpstream;

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        modal.hide();
      }
    };

    const toggleNode =
      (setter: Dispatch<SetStateAction<Set<string>>>, id: string) => () => {
        setter((prev) => {
          const next = new Set(prev);
          if (next.has(id)) {
            next.delete(id);
          } else {
            next.add(id);
          }
          return next;
        });
      };

    const renderTree = (
      nodes: TaskDependencyTreeNode[],
      expandedIds: Set<string>,
      setExpanded: Dispatch<SetStateAction<Set<string>>>,
      depth = 0
    ) => (
      <div className="space-y-1">
        {nodes.map((node) => {
          const hasChildren = node.dependencies.length > 0;
          const isExpanded = expandedIds.has(node.task.id);
          const statusTone = statusToneClasses[node.task.status];

          return (
            <div key={node.task.id}>
              <div
                className="flex items-center gap-2 py-1"
                style={{ paddingLeft: `${depth * 16}px` }}
              >
                {hasChildren ? (
                  <button
                    type="button"
                    onClick={toggleNode(setExpanded, node.task.id)}
                    className="flex h-5 w-5 items-center justify-center rounded-sm text-muted-foreground hover:text-foreground"
                    aria-label={
                      isExpanded
                        ? t('dependencyTreeDialog.collapse')
                        : t('dependencyTreeDialog.expand')
                    }
                    aria-expanded={isExpanded}
                  >
                    {isExpanded ? (
                      <ChevronDown className="h-4 w-4" />
                    ) : (
                      <ChevronRight className="h-4 w-4" />
                    )}
                  </button>
                ) : (
                  <span className="h-5 w-5" aria-hidden="true" />
                )}
                <span className={cn('h-2 w-2 rounded-full', statusTone.dot)} />
                <span
                  className="text-sm font-medium truncate"
                  title={node.task.title || ''}
                >
                  {node.task.title || '--'}
                </span>
                <span className={cn('text-xs', statusTone.text)}>
                  {statusLabels[node.task.status]}
                </span>
                {node.task.status === 'done' && (
                  <Check className="h-3 w-3 text-emerald-500" />
                )}
              </div>
              {hasChildren && isExpanded && (
                <div>
                  {renderTree(
                    node.dependencies,
                    expandedIds,
                    setExpanded,
                    depth + 1
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    );

    const renderSectionContent = (
      nodes: TaskDependencyTreeNode[] | undefined,
      isLoading: boolean,
      isError: boolean,
      emptyLabel: string,
      onRetry: () => void,
      expandedIds: Set<string>,
      setExpanded: Dispatch<SetStateAction<Set<string>>>
    ) => {
      if (isLoading) {
        return (
          <div className="text-sm text-muted-foreground">
            {t('dependencyTreeDialog.loading')}
          </div>
        );
      }

      if (isError) {
        return (
          <div className="space-y-3">
            <div className="text-sm text-destructive">
              {t('dependencyTreeDialog.error')}
            </div>
            <Button variant="outline" size="sm" onClick={onRetry}>
              {t('common:buttons.retry')}
            </Button>
          </div>
        );
      }

      if (!nodes || nodes.length === 0) {
        return (
          <div className="text-sm text-muted-foreground">{emptyLabel}</div>
        );
      }

      return renderTree(nodes, expandedIds, setExpanded);
    };

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={handleOpenChange}
        className="max-w-3xl w-[92vw] p-0 overflow-hidden"
      >
        <DialogContent className="p-0 min-w-0">
          <DialogHeader className="px-4 py-3 border-b">
            <DialogTitle>{t('dependencyTreeDialog.title')}</DialogTitle>
          </DialogHeader>

          <div className="p-4 max-h-[70vh] overflow-auto">
            <div className="rounded-lg border divide-y">
              <section className="p-4 space-y-3">
                <div className="text-xs uppercase tracking-wide text-muted-foreground">
                  {t('dependencyTreeDialog.blockedBy')}
                </div>
                {renderSectionContent(
                  upstreamTree?.dependencies,
                  isUpstreamLoading,
                  isUpstreamError,
                  t('dependencyTreeDialog.emptyBlockedBy'),
                  refetchUpstream,
                  expandedUpstream,
                  setExpandedUpstream
                )}
              </section>

              <section className="p-4 bg-muted/20">
                <div className="text-xs uppercase tracking-wide text-muted-foreground mb-2">
                  {t('dependencyTreeDialog.currentTask')}
                </div>
                {currentTask ? (
                  <div className="flex items-center gap-2 rounded-md border border-dashed border-muted-foreground/40 bg-background px-3 py-2">
                    <Star className="h-4 w-4 text-amber-500" />
                    <span
                      className="text-sm font-semibold truncate"
                      title={currentTask.title || ''}
                    >
                      {currentTask.title || '--'}
                    </span>
                    <span
                      className={cn(
                        'text-xs',
                        statusToneClasses[currentTask.status].text
                      )}
                    >
                      {statusLabels[currentTask.status]}
                    </span>
                  </div>
                ) : (
                  <div className="text-sm text-muted-foreground">
                    {isUpstreamError
                      ? t('dependencyTreeDialog.error')
                      : t('dependencyTreeDialog.loading')}
                  </div>
                )}
              </section>

              <section className="p-4 space-y-3">
                <div className="text-xs uppercase tracking-wide text-muted-foreground">
                  {t('dependencyTreeDialog.blocks')}
                </div>
                {renderSectionContent(
                  downstreamTree?.dependencies,
                  downstreamLoading,
                  downstreamError,
                  t('dependencyTreeDialog.emptyBlocks'),
                  retryDownstream,
                  expandedDownstream,
                  setExpandedDownstream
                )}
              </section>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  }
);

export const DependencyTreeDialog = defineModal<
  DependencyTreeDialogProps,
  void
>(DependencyTreeDialogImpl);
