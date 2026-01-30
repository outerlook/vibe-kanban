import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { useAuth } from '@/hooks';
import {
  type DragEndEvent,
  KanbanBoard,
  KanbanCards,
  KanbanHeader,
  KanbanProvider,
} from '@/components/ui/shadcn-io/kanban';
import { TaskCard } from './TaskCard';
import type { TaskStatus, TaskWithAttemptStatus } from 'shared/types';
import { statusBoardColors, statusLabels } from '@/utils/statusLabels';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { SharedTaskCard } from './SharedTaskCard';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import { Button } from '@/components/ui/button';
import { Loader2, ClipboardList } from 'lucide-react';
import { MobileKanbanCarousel } from './MobileKanbanCarousel';

export type KanbanColumnItem =
  | {
      type: 'task';
      task: TaskWithAttemptStatus;
      sharedTask?: SharedTaskRecord;
    }
  | {
      type: 'shared';
      task: SharedTaskRecord;
    };

export type KanbanColumns = Record<TaskStatus, KanbanColumnItem[]>;

interface TaskKanbanBoardProps {
  columns: KanbanColumns;
  onDragEnd: (event: DragEndEvent) => void;
  onViewTaskDetails: (task: TaskWithAttemptStatus) => void;
  onViewSharedTask?: (task: SharedTaskRecord) => void;
  selectedTaskId?: string;
  selectedSharedTaskId?: string | null;
  onCreateTask?: () => void;
  projectId: string;
  loadMoreByStatus: Record<TaskStatus, () => void>;
  hasMoreByStatus: Record<TaskStatus, boolean>;
  isLoadingMoreByStatus: Record<TaskStatus, boolean>;
  totalByStatus: Record<TaskStatus, number>;
  isMobile?: boolean;
  onLongPressTask?: (task: TaskWithAttemptStatus) => void;
}

function TaskKanbanBoard({
  columns,
  onDragEnd,
  onViewTaskDetails,
  onViewSharedTask,
  selectedTaskId,
  selectedSharedTaskId,
  onCreateTask,
  projectId,
  loadMoreByStatus,
  hasMoreByStatus,
  isLoadingMoreByStatus,
  totalByStatus,
  isMobile,
  onLongPressTask,
}: TaskKanbanBoardProps) {
  const { t } = useTranslation(['tasks']);
  const { userId } = useAuth();
  const { isTaskSelected } = useTaskSelection();

  if (isMobile && onLongPressTask) {
    return (
      <MobileKanbanCarousel
        columns={columns}
        onViewTaskDetails={onViewTaskDetails}
        onViewSharedTask={onViewSharedTask}
        selectedTaskId={selectedTaskId}
        selectedSharedTaskId={selectedSharedTaskId}
        onCreateTask={onCreateTask}
        projectId={projectId}
        loadMoreByStatus={loadMoreByStatus}
        hasMoreByStatus={hasMoreByStatus}
        isLoadingMoreByStatus={isLoadingMoreByStatus}
        totalByStatus={totalByStatus}
        onLongPressTask={onLongPressTask}
      />
    );
  }

  return (
    <KanbanProvider onDragEnd={onDragEnd}>
      {Object.entries(columns).map(([status, items]) => {
        const statusKey = status as TaskStatus;
        const hasMore = hasMoreByStatus[statusKey];
        const isLoadingMore = isLoadingMoreByStatus[statusKey];
        const total = totalByStatus[statusKey];
        const loadMore = loadMoreByStatus[statusKey];

        return (
          <KanbanBoard key={status} id={statusKey}>
            <KanbanHeader
              name={statusLabels[statusKey]}
              color={statusBoardColors[statusKey]}
              onAddTask={onCreateTask}
            />
            <KanbanCards>
              {items.length === 0 && (
                <div className="flex flex-col items-center justify-center py-8 text-center opacity-50">
                  <div className="mb-2 p-2 bg-muted/50 rounded-full">
                    <ClipboardList className="h-4 w-4" />
                  </div>
                  <p className="text-xs font-medium text-muted-foreground">
                    {t('empty.noTasks', { defaultValue: 'No tasks' })}
                  </p>
                </div>
              )}
              {items.map((item, index) => {
                const isOwnTask =
                  item.type === 'task' &&
                  (!item.sharedTask?.assignee_user_id ||
                    !userId ||
                    item.sharedTask?.assignee_user_id === userId);

                if (isOwnTask) {
                  return (
                    <TaskCard
                      key={item.task.id}
                      task={item.task}
                      index={index}
                      status={statusKey}
                      onViewDetails={onViewTaskDetails}
                      isOpen={selectedTaskId === item.task.id}
                      isSelected={isTaskSelected(item.task.id)}
                      projectId={projectId}
                      sharedTask={item.sharedTask}
                    />
                  );
                }

                const sharedTask =
                  item.type === 'shared' ? item.task : item.sharedTask!;

                return (
                  <SharedTaskCard
                    key={`shared-${item.task.id}`}
                    task={sharedTask}
                    index={index}
                    status={statusKey}
                    isSelected={selectedSharedTaskId === item.task.id}
                    onViewDetails={onViewSharedTask}
                  />
                );
              })}
              {hasMore && (
                <div className="flex flex-col items-center gap-1 py-2 px-2">
                  <Button
                    onClick={loadMore}
                    disabled={isLoadingMore}
                    variant="ghost"
                    size="sm"
                    className="w-full text-xs"
                  >
                    {isLoadingMore && (
                      <Loader2 className="h-3 w-3 animate-spin mr-1" />
                    )}
                    {t('pagination.showMore', { defaultValue: 'Show more' })}
                  </Button>
                  <span className="text-[10px] text-muted-foreground">
                    {t('pagination.showingOfTotal', {
                      defaultValue: '{{count}} of {{total}}',
                      count: items.length,
                      total,
                    })}
                  </span>
                </div>
              )}
            </KanbanCards>
          </KanbanBoard>
        );
      })}
    </KanbanProvider>
  );
}

export default memo(TaskKanbanBoard);
