import { useState } from 'react';
import { Circle, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import { useTaskMutations } from '@/hooks';
import { statusLabels, statusBoardColors } from '@/utils/statusLabels';
import type { TaskWithAttemptStatus, TaskStatus } from 'shared/types';

type Props = {
  task: TaskWithAttemptStatus;
  disabled?: boolean;
  className?: string;
};

const allStatuses: TaskStatus[] = [
  'todo',
  'inprogress',
  'inreview',
  'done',
  'cancelled',
];

export function TaskStatusSelector({ task, disabled, className }: Props) {
  const [isUpdating, setIsUpdating] = useState(false);
  const { updateTask } = useTaskMutations();

  const handleStatusChange = async (newStatus: string) => {
    if (newStatus === task.status) return;

    setIsUpdating(true);
    try {
      await updateTask.mutateAsync({
        taskId: task.id,
        data: {
          title: null,
          description: null,
          status: newStatus as TaskStatus,
          parent_workspace_id: null,
          image_ids: null,
        },
      });
    } finally {
      setIsUpdating(false);
    }
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="secondary"
          size="sm"
          className={cn('px-2 flex items-center gap-1.5', className)}
          disabled={disabled || isUpdating}
        >
          <Circle
            className="h-2.5 w-2.5 fill-current"
            style={{ color: `var(${statusBoardColors[task.status]})` }}
          />
          <span className="text-xs">{statusLabels[task.status]}</span>
          <ChevronDown className="h-3 w-3 ml-0.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuRadioGroup
          value={task.status}
          onValueChange={handleStatusChange}
        >
          {allStatuses.map((status) => (
            <DropdownMenuRadioItem key={status} value={status}>
              <Circle
                className="h-2.5 w-2.5 fill-current mr-2"
                style={{ color: `var(${statusBoardColors[status]})` }}
              />
              {statusLabels[status]}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
