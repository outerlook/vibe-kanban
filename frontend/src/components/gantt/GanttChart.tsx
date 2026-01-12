import { useRef, useCallback } from 'react';
import { Gantt } from '@svar-ui/react-gantt';
import type { IApi, ITask, TID } from '@svar-ui/react-gantt';
import type { SvarGanttTask, SvarGanttLink } from '@/lib/transformGantt';
import { GANTT_SCALES } from '@/lib/ganttConfig';
import '@/styles/gantt.css';

/**
 * Task type configuration for SVAR Gantt.
 * Each type ID matches a TaskStatus value and defines bar colors.
 */
const TASK_TYPES = [
  { id: 'todo', label: 'To Do' },
  { id: 'inprogress', label: 'In Progress' },
  { id: 'inreview', label: 'In Review' },
  { id: 'done', label: 'Done' },
  { id: 'cancelled', label: 'Cancelled' },
];

interface GanttChartProps {
  tasks: SvarGanttTask[];
  links: SvarGanttLink[];
  onSelectTask?: (taskId: string) => void;
}

export function GanttChart({
  tasks,
  links,
  onSelectTask,
}: GanttChartProps) {
  const apiRef = useRef<IApi | null>(null);

  const handleInit = useCallback(
    (api: IApi) => {
      apiRef.current = api;

      api.on('select-task', (ev: { id: TID }) => {
        onSelectTask?.(String(ev.id));
      });
    },
    [onSelectTask]
  );

  const taskTemplate = useCallback(
    ({
      data,
    }: {
      data: ITask;
      api: IApi;
      onaction: (ev: { action: string; data: Record<string, unknown> }) => void;
    }) => {
      return <span className="wx-gantt-task-text">{data.text}</span>;
    },
    []
  );

  if (tasks.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        No tasks to display
      </div>
    );
  }

  return (
    <div className="gantt-container w-full h-full overflow-auto">
      <Gantt
        tasks={tasks}
        links={links}
        taskTypes={TASK_TYPES}
        scales={GANTT_SCALES}
        init={handleInit}
        readonly={true}
        lengthUnit="minute"
        columns={[]}
        taskTemplate={taskTemplate}
      />
    </div>
  );
}
