import { ClipboardList } from 'lucide-react';
import { Skeleton } from '@/components/ui/skeleton';
import { InReviewTaskItem } from './InReviewTaskItem';
import type { TaskWithAttemptStatus } from 'shared/types';

interface InReviewTasksListProps {
  tasks: (TaskWithAttemptStatus & { projectName?: string })[];
  isLoading: boolean;
  onClose?: () => void;
}

function LoadingSkeleton() {
  return (
    <div className="flex flex-col">
      {[...Array(4)].map((_, i) => (
        <div key={i} className="flex items-center gap-2 px-3 py-2 border-b">
          <Skeleton variant="text" className="flex-1" height={16} />
          <Skeleton variant="rectangular" width={60} height={20} />
        </div>
      ))}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center py-8 px-4 text-muted-foreground">
      <ClipboardList className="h-8 w-8 mb-2" />
      <p className="text-sm">No tasks in review</p>
    </div>
  );
}

function groupTasksByProject(
  tasks: (TaskWithAttemptStatus & { projectName?: string })[]
) {
  const groups = new Map<string, typeof tasks>();

  for (const task of tasks) {
    const projectName = task.projectName ?? 'Unknown Project';
    const existing = groups.get(projectName) ?? [];
    existing.push(task);
    groups.set(projectName, existing);
  }

  return groups;
}

export function InReviewTasksList({
  tasks,
  isLoading,
  onClose,
}: InReviewTasksListProps) {
  if (isLoading) {
    return (
      <div className="flex flex-col">
        <div className="flex items-center justify-between px-3 py-2 border-b">
          <span className="text-sm font-medium">Tasks in Review</span>
        </div>
        <LoadingSkeleton />
      </div>
    );
  }

  if (tasks.length === 0) {
    return (
      <div className="flex flex-col">
        <div className="flex items-center justify-between px-3 py-2 border-b">
          <span className="text-sm font-medium">Tasks in Review</span>
        </div>
        <EmptyState />
      </div>
    );
  }

  const groupedTasks = groupTasksByProject(tasks);

  return (
    <div className="flex flex-col">
      <div className="flex items-center justify-between px-3 py-2 border-b">
        <span className="text-sm font-medium">Tasks in Review</span>
        <span className="text-xs text-muted-foreground">{tasks.length}</span>
      </div>

      <div className="max-h-[400px] overflow-y-auto">
        {[...groupedTasks.entries()].map(([projectName, projectTasks]) => (
          <div key={projectName}>
            <div className="px-3 py-1.5 bg-muted/50 text-xs font-medium text-muted-foreground sticky top-0">
              {projectName}
            </div>
            {projectTasks.map((task) => (
              <InReviewTaskItem key={task.id} task={task} onClose={onClose} />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
