import { useTranslation } from 'react-i18next';
import { Circle, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { TASK_STATUSES } from '@/constants/taskStatuses';
import { cn } from '@/lib/utils';
import { useTaskMutations } from '@/hooks';
import { getTaskStatusLabel, statusBoardColors } from '@/utils/statusLabels';
import type { TaskWithAttemptStatus, TaskStatus } from 'shared/types';

type Props = {
  task: TaskWithAttemptStatus;
  disabled?: boolean;
  className?: string;
};

export function TaskStatusSelector({ task, disabled, className }: Props) {
  const { t } = useTranslation('tasks');
  const { updateTask } = useTaskMutations();

  const handleStatusChange = (newStatus: string) => {
    if (newStatus === task.status) return;

    updateTask.mutate({
      taskId: task.id,
      data: {
        title: null,
        description: null,
        status: newStatus as TaskStatus,
        parent_workspace_id: null,
        image_ids: null,
        task_group_id: null,
      },
    });
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="secondary"
          size="sm"
          className={cn('px-2 flex items-center gap-1.5', className)}
          disabled={disabled || updateTask.isPending}
        >
          <Circle
            className="h-2.5 w-2.5 fill-current"
            style={{ color: `var(${statusBoardColors[task.status]})` }}
          />
          <span className="text-xs">{getTaskStatusLabel(t, task.status)}</span>
          <ChevronDown className="h-3 w-3 ml-0.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuRadioGroup
          value={task.status}
          onValueChange={handleStatusChange}
        >
          {TASK_STATUSES.map((status) => (
            <DropdownMenuRadioItem key={status} value={status}>
              <Circle
                className="h-2.5 w-2.5 fill-current mr-2"
                style={{ color: `var(${statusBoardColors[status]})` }}
              />
              {getTaskStatusLabel(t, status)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
