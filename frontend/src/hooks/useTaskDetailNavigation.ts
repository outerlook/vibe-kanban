import type { LayoutMode } from '@/components/layout/TasksLayout';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import type { TaskWithAttemptStatus, Workspace } from 'shared/types';

export interface TaskDetailNavigation {
  task: TaskWithAttemptStatus | null;
  taskId?: string;
  isTaskView: boolean;
  attempt?: Workspace | null;
  mode?: LayoutMode;
  onModeChange?: (mode: LayoutMode) => void;
  sharedTask?: SharedTaskRecord;
  onClose: () => void;
  navigateWithSearch?: (path: string, options?: { replace?: boolean }) => void;
}
