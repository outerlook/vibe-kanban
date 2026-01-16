import { useRef, useCallback } from 'react';
import { Gantt, Tooltip } from '@svar-ui/react-gantt';
import type { IApi, ITask, TID } from '@svar-ui/react-gantt';
import { GanttTooltipContent } from './GanttTooltipContent';
import type { SvarGanttTask, SvarGanttLink } from '@/lib/transformGantt';
import { GANTT_SCALES } from '@/lib/ganttConfig';
import '@/styles/gantt.css';

/**
 * Task type configuration for SVAR Gantt.
 * Each type ID matches a TaskStatus value or group color class and defines bar colors.
 * Status types: todo, inprogress, inreview, done, cancelled
 * Group types: ungrouped, group-0 through group-9
 */
const TASK_TYPES = [
  // Status-based types
  { id: 'todo', label: 'To Do' },
  { id: 'inprogress', label: 'In Progress' },
  { id: 'inreview', label: 'In Review' },
  { id: 'done', label: 'Done' },
  { id: 'cancelled', label: 'Cancelled' },
  // Group-based types (colors applied via CSS classes in gantt.css)
  { id: 'ungrouped', label: 'Ungrouped' },
  { id: 'group-0', label: 'Group 1' },
  { id: 'group-1', label: 'Group 2' },
  { id: 'group-2', label: 'Group 3' },
  { id: 'group-3', label: 'Group 4' },
  { id: 'group-4', label: 'Group 5' },
  { id: 'group-5', label: 'Group 6' },
  { id: 'group-6', label: 'Group 7' },
  { id: 'group-7', label: 'Group 8' },
  { id: 'group-8', label: 'Group 9' },
  { id: 'group-9', label: 'Group 10' },
];

interface GanttChartProps {
  tasks: SvarGanttTask[];
  links: SvarGanttLink[];
  onSelectTask?: (taskId: string) => void;
}

export function GanttChart({ tasks, links, onSelectTask }: GanttChartProps) {
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
    ({ data }: { data: ITask }) => (
      <span className="wx-gantt-task-text">{data.text}</span>
    ),
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
      <Tooltip api={apiRef.current ?? undefined} content={GanttTooltipContent}>
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
      </Tooltip>
    </div>
  );
}
