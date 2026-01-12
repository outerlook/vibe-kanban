import { useRef, useCallback } from 'react';
import { Gantt, Tooltip } from '@svar-ui/react-gantt';
import type { IApi, ITask, TID } from '@svar-ui/react-gantt';
import type { SvarGanttTask, SvarGanttLink } from '@/lib/transformGantt';
import { GANTT_SCALES } from '@/lib/ganttConfig';
import { formatTokenCount } from '@/lib/utils';
import { useNavigateWithSearch } from '@/hooks';
import { paths } from '@/lib/paths';
import '@/styles/gantt.css';

function TooltipContent({ data }: { data: ITask }) {
  const task = data as unknown as SvarGanttTask;
  const hasTokens =
    task.totalInputTokens != null || task.totalOutputTokens != null;

  return (
    <div className="p-2 text-sm">
      <div className="font-semibold mb-1">{task.text}</div>
      <div className="text-muted-foreground text-xs space-y-0.5">
        <div>Status: {task.type}</div>
        <div>Progress: {Math.round(task.progress * 100)}%</div>
        {hasTokens && (
          <div>
            Tokens: {formatTokenCount(task.totalInputTokens) || '0'} /{' '}
            {formatTokenCount(task.totalOutputTokens) || '0'}
          </div>
        )}
      </div>
    </div>
  );
}

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
  projectId: string;
  tasks: SvarGanttTask[];
  links: SvarGanttLink[];
}

export function GanttChart({ projectId, tasks, links }: GanttChartProps) {
  const apiRef = useRef<IApi | null>(null);
  const navigate = useNavigateWithSearch();

  const handleInit = useCallback(
    (api: IApi) => {
      apiRef.current = api;

      api.on('select-task', (ev: { id: TID }) => {
        navigate(paths.task(projectId, String(ev.id)));
      });
    },
    [navigate, projectId]
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
      <Tooltip api={apiRef.current ?? undefined} content={TooltipContent}>
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
