import { memo, useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Carousel,
  CarouselContent,
  CarouselItem,
  type CarouselApi,
} from '@/components/ui/carousel';
import { ColumnIndicators } from './ColumnIndicators';
import { TaskCard } from './TaskCard';
import { SharedTaskCard } from './SharedTaskCard';
import { Button } from '@/components/ui/button';
import { ClipboardList, Loader2 } from 'lucide-react';
import { TASK_STATUSES } from '@/constants/taskStatuses';
import { statusLabels, statusBoardColors } from '@/utils/statusLabels';
import { useAuth } from '@/hooks';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import type { TaskStatus, TaskWithAttemptStatus } from 'shared/types';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import type { KanbanColumns } from './TaskKanbanBoard';

interface MobileKanbanCarouselProps {
  columns: KanbanColumns;
  onViewTaskDetails: (task: TaskWithAttemptStatus) => void;
  onViewSharedTask?: (task: SharedTaskRecord) => void;
  selectedTaskId?: string;
  selectedSharedTaskId?: string | null;
  projectId: string;
  loadMoreByStatus: Record<TaskStatus, () => void>;
  hasMoreByStatus: Record<TaskStatus, boolean>;
  isLoadingMoreByStatus: Record<TaskStatus, boolean>;
  totalByStatus: Record<TaskStatus, number>;
  onLongPressTask: (task: TaskWithAttemptStatus) => void;
}

function MobileKanbanCarouselComponent({
  columns,
  onViewTaskDetails,
  onViewSharedTask,
  selectedTaskId,
  selectedSharedTaskId,
  projectId,
  loadMoreByStatus,
  hasMoreByStatus,
  isLoadingMoreByStatus,
  totalByStatus,
  onLongPressTask,
}: MobileKanbanCarouselProps) {
  const { t } = useTranslation(['tasks']);
  const { userId } = useAuth();
  const { isTaskSelected } = useTaskSelection();
  const [api, setApi] = useState<CarouselApi>();
  const [currentIndex, setCurrentIndex] = useState(0);

  useEffect(() => {
    if (!api) return;

    const onSelect = () => {
      setCurrentIndex(api.selectedScrollSnap());
    };

    api.on('select', onSelect);
    onSelect();

    return () => {
      api.off('select', onSelect);
    };
  }, [api]);

  const handleColumnSelect = useCallback(
    (index: number) => {
      api?.scrollTo(index);
    },
    [api]
  );

  return (
    <div className="flex flex-col h-full">
      <ColumnIndicators
        currentIndex={currentIndex}
        onColumnSelect={handleColumnSelect}
      />

      <Carousel
        setApi={setApi}
        opts={{
          align: 'start',
          loop: false,
        }}
        className="flex-1 min-h-0"
      >
        <CarouselContent className="h-full">
          {TASK_STATUSES.map((status) => {
            const items = columns[status];
            const hasMore = hasMoreByStatus[status];
            const isLoadingMore = isLoadingMoreByStatus[status];
            const total = totalByStatus[status];
            const loadMore = loadMoreByStatus[status];
            const colorVar = statusBoardColors[status];

            return (
              <CarouselItem key={status} className="h-full">
                <div className="flex flex-col h-full">
                  <div className="flex items-center gap-2 px-4 py-2 border-b">
                    <span
                      className="w-2 h-2 rounded-full"
                      style={{ backgroundColor: `var(${colorVar})` }}
                    />
                    <span className="font-medium text-sm">
                      {statusLabels[status]}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      ({total})
                    </span>
                  </div>

                  <div className="flex-1 overflow-y-auto px-4 py-2 space-y-2">
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
                            status={status}
                            onViewDetails={onViewTaskDetails}
                            isOpen={selectedTaskId === item.task.id}
                            isSelected={isTaskSelected(item.task.id)}
                            projectId={projectId}
                            sharedTask={item.sharedTask}
                            isMobile
                            onLongPress={() => onLongPressTask(item.task)}
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
                          status={status}
                          isSelected={selectedSharedTaskId === item.task.id}
                          onViewDetails={onViewSharedTask}
                        />
                      );
                    })}

                    {hasMore && (
                      <div className="flex flex-col items-center gap-1 py-2">
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
                          {t('pagination.showMore', {
                            defaultValue: 'Show more',
                          })}
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
                  </div>
                </div>
              </CarouselItem>
            );
          })}
        </CarouselContent>
      </Carousel>
    </div>
  );
}

export const MobileKanbanCarousel = memo(MobileKanbanCarouselComponent);
