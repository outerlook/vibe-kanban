import { useEffect, useRef, useCallback } from 'react';
import { Gantt } from '@svar-ui/react-gantt';
import type { IApi, ITask, TID } from '@svar-ui/react-gantt';
import type { SvarGanttTask, SvarGanttLink } from '@/lib/transformGantt';
import {
  GANTT_ZOOM_CONFIG,
  viewModeToZoomLevel,
  type GanttViewMode,
} from '@/lib/ganttConfig';
import { useNavigateWithSearch } from '@/hooks';
import { paths } from '@/lib/paths';
import '@/styles/gantt.css';

export type { GanttViewMode };

interface GanttChartProps {
  projectId: string;
  tasks: SvarGanttTask[];
  links: SvarGanttLink[];
  viewMode?: GanttViewMode;
}

export function GanttChart({
  projectId,
  tasks,
  links,
  viewMode = 'Week',
}: GanttChartProps) {
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

  useEffect(() => {
    if (apiRef.current) {
      const zoomLevel = viewModeToZoomLevel(viewMode);
      apiRef.current.exec('set-zoom', { level: zoomLevel });
    }
  }, [viewMode]);

  const taskTemplate = useCallback(
    ({
      data,
    }: {
      data: ITask;
      api: IApi;
      onaction: (ev: { action: string; data: Record<string, unknown> }) => void;
    }) => {
      const svarTask = data as unknown as SvarGanttTask;
      const statusClass = `gantt-task-${svarTask.taskStatus}`;
      return (
        <div className={`wx-gantt-task-content ${statusClass}`}>
          {svarTask.text}
        </div>
      );
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
        zoom={{
          level: viewModeToZoomLevel(viewMode),
          levels: GANTT_ZOOM_CONFIG,
        }}
        init={handleInit}
        readonly={true}
        durationUnit="day"
        lengthUnit="hour"
        columns={[]}
        taskTemplate={taskTemplate}
      />
    </div>
  );
}
