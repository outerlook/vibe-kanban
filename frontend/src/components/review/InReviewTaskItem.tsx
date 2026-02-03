import { useNavigate } from 'react-router-dom';
import { cn } from '@/lib/utils';
import { paths } from '@/lib/paths';
import type { TaskWithAttemptStatus } from 'shared/types';

interface InReviewTaskItemProps {
  task: TaskWithAttemptStatus & { projectName?: string };
  onClose?: () => void;
}

export function InReviewTaskItem({ task, onClose }: InReviewTaskItemProps) {
  const navigate = useNavigate();

  const handleClick = () => {
    navigate(paths.task(task.project_id, task.id));
    onClose?.();
  };

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          handleClick();
        }
      }}
      className={cn(
        'flex items-center gap-2 px-3 py-2 cursor-pointer',
        'hover:bg-accent transition-colors',
        'border-b last:border-b-0'
      )}
    >
      <p className="text-sm line-clamp-1 flex-1 min-w-0">{task.title}</p>
      {task.projectName && (
        <span className="text-xs bg-muted px-1.5 py-0.5 rounded shrink-0">
          {task.projectName}
        </span>
      )}
    </div>
  );
}
