import { useNavigate } from 'react-router-dom';
import { cn } from '@/lib/utils';
import { paths } from '@/lib/paths';
import type { TaskWithAttemptStatus } from 'shared/types';

interface InReviewTaskItemProps {
  task: TaskWithAttemptStatus;
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
        'px-3 py-2 cursor-pointer',
        'hover:bg-accent transition-colors',
        'border-b last:border-b-0'
      )}
    >
      <p className="text-sm line-clamp-1">{task.title}</p>
    </div>
  );
}
