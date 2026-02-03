import { useMemo, useState } from 'react';
import { ClipboardCheck } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useProjects } from '@/hooks/useProjects';
import { tasksApi } from '@/lib/api';
import { InReviewTasksList } from './InReviewTasksList';
import { cn } from '@/lib/utils';
import type { TaskWithAttemptStatus } from 'shared/types';

interface InReviewTasksBadgeProps {
  className?: string;
}

export function InReviewTasksBadge({ className }: InReviewTasksBadgeProps) {
  const [open, setOpen] = useState(false);
  const { projects } = useProjects();

  const { totalCount, projectsWithReviewTasks } = useMemo(() => {
    let total = 0;
    const withTasks: { id: string; name: string; count: number }[] = [];

    for (const project of projects) {
      const count = Number(project.task_counts.inreview);
      if (count > 0) {
        total += count;
        withTasks.push({ id: project.id, name: project.name, count });
      }
    }

    return { totalCount: total, projectsWithReviewTasks: withTasks };
  }, [projects]);

  const { data: tasks = [], isLoading } = useQuery({
    queryKey: ['inreview-tasks-badge', projectsWithReviewTasks.map((p) => p.id)],
    queryFn: async () => {
      const results = await Promise.all(
        projectsWithReviewTasks.map(async (project) => {
          const response = await tasksApi.list(project.id, {
            status: 'inreview',
            limit: 100,
          });
          return response.tasks.map((task) => ({
            ...task,
            projectName: project.name,
          }));
        })
      );
      return results.flat();
    },
    enabled: open && projectsWithReviewTasks.length > 0,
    staleTime: 30_000,
  });

  const typedTasks: (TaskWithAttemptStatus & { projectName?: string })[] = tasks;

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className={cn('h-9 w-9 relative', className)}
          aria-label={`Tasks in review (${totalCount} tasks)`}
        >
          <ClipboardCheck className="h-4 w-4" />
          {totalCount > 0 && (
            <span
              className={cn(
                'absolute -top-0.5 -right-0.5 flex items-center justify-center',
                'min-w-[18px] h-[18px] px-1 rounded-full',
                'bg-amber-500 text-white text-[10px] font-medium'
              )}
            >
              {totalCount > 99 ? '99+' : totalCount}
            </span>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="w-[380px] p-0"
        sideOffset={8}
      >
        <InReviewTasksList
          tasks={typedTasks}
          isLoading={isLoading}
          onClose={() => setOpen(false)}
        />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
