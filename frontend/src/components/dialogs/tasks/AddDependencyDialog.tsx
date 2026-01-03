import { useEffect, useMemo, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { DataTable, type ColumnDef } from '@/components/ui/table/data-table';
import { useAddDependency } from '@/hooks';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { statusLabels } from '@/utils/statusLabels';
import { ApiError } from '@/lib/api';
import { Loader2, Search } from 'lucide-react';
import type { TaskWithAttemptStatus } from 'shared/types';

export interface AddDependencyDialogProps {
  taskId: string;
  projectId: string;
  existingDependencyIds: string[];
  dependenciesLoading?: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const getReadableError = (error: unknown) => {
  if (error instanceof ApiError) {
    const message = error.message || 'Failed to add dependency.';
    if (error.status === 400 && message.toLowerCase().includes('cycle')) {
      return message;
    }
    if (error.status === 409) {
      return 'This task is already a dependency.';
    }
    return message;
  }

  if (error instanceof Error) {
    return error.message || 'Failed to add dependency.';
  }

  return 'Failed to add dependency.';
};

export function AddDependencyDialog({
  taskId,
  projectId,
  existingDependencyIds,
  dependenciesLoading = false,
  open,
  onOpenChange,
}: AddDependencyDialogProps) {
  const [search, setSearch] = useState('');
  const [submitError, setSubmitError] = useState<string | null>(null);
  const addDependency = useAddDependency();
  const isMutating = addDependency.isPending;

  const {
    tasks,
    isLoading,
    isLoadingMore,
    hasMore,
    loadMore,
    error: tasksError,
  } = useProjectTasks(projectId);

  useEffect(() => {
    if (!open) return;
    setSearch('');
    setSubmitError(null);
    addDependency.reset();
  }, [open, addDependency]);

  const normalizedSearch = search.trim().toLowerCase();
  const excludedIds = useMemo(
    () => new Set([taskId, ...existingDependencyIds]),
    [taskId, existingDependencyIds]
  );

  const availableTasks = useMemo(
    () =>
      tasks.filter((task) => {
        if (excludedIds.has(task.id)) {
          return false;
        }
        if (!normalizedSearch) {
          return true;
        }
        return task.title.toLowerCase().includes(normalizedSearch);
      }),
    [tasks, excludedIds, normalizedSearch]
  );

  const taskColumns: ColumnDef<TaskWithAttemptStatus>[] = [
    {
      id: 'title',
      header: 'Task',
      accessor: (task) => (
        <div className="truncate" title={task.title}>
          {task.title || 'Untitled task'}
        </div>
      ),
      className: 'pr-4',
      headerClassName: 'font-medium py-2 pr-4 bg-card',
    },
    {
      id: 'status',
      header: 'Status',
      accessor: (task) => (
        <Badge variant="outline">{statusLabels[task.status] || '—'}</Badge>
      ),
      headerClassName: 'font-medium py-2 bg-card w-32',
    },
  ];

  const handleAddDependency = async (dependsOnId: string) => {
    if (isMutating) {
      return;
    }

    setSubmitError(null);
    try {
      await addDependency.mutateAsync({ taskId, dependsOnId });
      onOpenChange(false);
    } catch (error: unknown) {
      setSubmitError(getReadableError(error));
    }
  };

  const emptyState = normalizedSearch
    ? 'No tasks match your search.'
    : 'No available tasks to add.';
  const isTableLoading = isLoading || dependenciesLoading;
  const canSelectTask = !isTableLoading && !isMutating;

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (isMutating) {
          return;
        }
        onOpenChange(nextOpen);
      }}
    >
      <DialogContent className="p-0">
        <DialogHeader className="px-4 py-3 border-b">
          <DialogTitle>Add dependency</DialogTitle>
          <DialogDescription>
            Search tasks in this project to add as dependencies.
          </DialogDescription>
        </DialogHeader>

        <div className="p-4 space-y-3">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search tasks..."
              className="pl-8"
              disabled={!projectId || isMutating || dependenciesLoading}
            />
          </div>

          {tasksError && (
            <Alert variant="destructive">
              <AlertDescription>{tasksError}</AlertDescription>
            </Alert>
          )}

          {submitError && (
            <Alert variant="destructive">
              <AlertDescription>{submitError}</AlertDescription>
            </Alert>
          )}

          <div className="border rounded-md max-h-[50vh] overflow-auto">
            <DataTable
              data={availableTasks}
              columns={taskColumns}
              keyExtractor={(task) => task.id}
              onRowClick={canSelectTask ? (task) => handleAddDependency(task.id) : undefined}
              isLoading={isTableLoading}
              emptyState={emptyState}
            />
          </div>

          {hasMore && (
            <div className="flex justify-center">
              <Button
                type="button"
                variant="secondary"
                onClick={loadMore}
                disabled={isLoadingMore || isMutating}
              >
                {isLoadingMore && (
                  <Loader2 className="h-4 w-4 animate-spin mr-2" />
                )}
                Load more
              </Button>
            </div>
          )}
        </div>

        <DialogFooter className="px-4 pb-4">
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isMutating}
          >
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
