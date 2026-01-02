import { useMemo, useState } from 'react';
import { AlertTriangle, Plus, X } from 'lucide-react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useAddDependency, useRemoveDependency, useTaskDependencies } from '@/hooks';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { cn } from '@/lib/utils';
import { statusLabels } from '@/utils/statusLabels';
import type { Task, TaskStatus } from 'shared/types';

type DependencySectionProps = {
  taskId: string;
  projectId: string;
};

const statusBadgeStyles: Record<TaskStatus, string> = {
  todo: 'border-muted-foreground/40 text-muted-foreground',
  inprogress: 'border-blue-500/40 text-blue-600',
  inreview: 'border-amber-500/40 text-amber-600',
  done: 'border-emerald-600/40 text-emerald-600',
  cancelled: 'border-destructive/40 text-destructive',
};

type DependencyItemProps = {
  task: Task;
  onRemove: () => void;
  isRemoving: boolean;
};

function DependencyItem({ task, onRemove, isRemoving }: DependencyItemProps) {
  return (
    <div
      className={cn(
        'flex items-center justify-between gap-3',
        'rounded-md border border-border px-2 py-1.5'
      )}
    >
      <div className="flex items-center gap-2 min-w-0">
        <span className="text-sm text-foreground truncate">{task.title}</span>
        <Badge
          variant="outline"
          className={cn('shrink-0', statusBadgeStyles[task.status])}
        >
          {statusLabels[task.status]}
        </Badge>
      </div>
      <Button
        variant="icon"
        size="sm"
        onClick={onRemove}
        disabled={isRemoving}
        aria-label="Remove dependency"
        className="h-7 w-7"
      >
        <X className="h-4 w-4" />
      </Button>
    </div>
  );
}

export function DependencySection({
  taskId,
  projectId,
}: DependencySectionProps) {
  const { data: deps, isLoading } = useTaskDependencies(taskId);
  const addDependency = useAddDependency();
  const removeDependency = useRemoveDependency();
  const { tasks } = useProjectTasks(projectId);
  const [selectedTaskId, setSelectedTaskId] = useState('');

  const blockedBy = deps?.blocked_by ?? [];
  const blocking = deps?.blocking ?? [];

  const isBlocked = blockedBy.some((task) => task.status !== 'done');

  const availableTasks = useMemo(() => {
    const relatedIds = new Set<string>([
      taskId,
      ...blockedBy.map((task) => task.id),
      ...blocking.map((task) => task.id),
    ]);

    return tasks.filter((task) => !relatedIds.has(task.id));
  }, [tasks, taskId, blockedBy, blocking]);

  const handleAddDependency = async () => {
    if (!selectedTaskId) return;
    await addDependency.mutateAsync({
      taskId,
      dependsOnId: selectedTaskId,
    });
    setSelectedTaskId('');
  };

  const handleRemoveBlockedBy = async (dependencyId: string) => {
    await removeDependency.mutateAsync({
      taskId,
      dependsOnId: dependencyId,
    });
  };

  const handleRemoveBlocking = async (blockedTaskId: string) => {
    await removeDependency.mutateAsync({
      taskId: blockedTaskId,
      dependsOnId: taskId,
    });
  };

  return (
    <div className="space-y-4">
      {isBlocked && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>Blocked</AlertTitle>
          <AlertDescription>
            This task is blocked by unfinished dependencies.
          </AlertDescription>
        </Alert>
      )}

      <div className="space-y-2">
        <h4 className="text-sm font-semibold">Blocked by</h4>
        {isLoading ? (
          <div className="text-sm text-muted-foreground">Loading...</div>
        ) : blockedBy.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            No blocking tasks.
          </div>
        ) : (
          <div className="space-y-2">
            {blockedBy.map((task) => (
              <DependencyItem
                key={task.id}
                task={task}
                onRemove={() => handleRemoveBlockedBy(task.id)}
                isRemoving={removeDependency.isPending}
              />
            ))}
          </div>
        )}
      </div>

      <div className="space-y-2">
        <h4 className="text-sm font-semibold">Blocks</h4>
        {isLoading ? (
          <div className="text-sm text-muted-foreground">Loading...</div>
        ) : blocking.length === 0 ? (
          <div className="text-sm text-muted-foreground">No blocked tasks.</div>
        ) : (
          <div className="space-y-2">
            {blocking.map((task) => (
              <DependencyItem
                key={task.id}
                task={task}
                onRemove={() => handleRemoveBlocking(task.id)}
                isRemoving={removeDependency.isPending}
              />
            ))}
          </div>
        )}
      </div>

      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <Select value={selectedTaskId} onValueChange={setSelectedTaskId}>
          <SelectTrigger className="sm:max-w-[20rem]">
            <SelectValue placeholder="Select task" />
          </SelectTrigger>
          <SelectContent>
            {availableTasks.length === 0 ? (
              <SelectItem value="none" disabled>
                No available tasks
              </SelectItem>
            ) : (
              availableTasks.map((task) => (
                <SelectItem key={task.id} value={task.id}>
                  {task.title}
                </SelectItem>
              ))
            )}
          </SelectContent>
        </Select>
        <Button
          variant="outline"
          size="sm"
          onClick={handleAddDependency}
          disabled={!selectedTaskId || addDependency.isPending}
          className="gap-2"
        >
          <Plus className="h-4 w-4" />
          Add dependency
        </Button>
      </div>
    </div>
  );
}
