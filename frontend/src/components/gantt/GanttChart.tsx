import { useEffect, useRef } from 'react';
import Gantt from 'frappe-gantt';
import type { FrappeGanttTask } from '@/lib/transformGantt';
import { useNavigateWithSearch } from '@/hooks';
import { paths } from '@/lib/paths';

export type GanttViewMode = 'Quarter Day' | 'Half Day' | 'Day' | 'Week' | 'Month';

interface GanttChartProps {
  projectId: string;
  tasks: FrappeGanttTask[];
  viewMode?: GanttViewMode;
}

export function GanttChart({
  projectId,
  tasks,
  viewMode = 'Week',
}: GanttChartProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const ganttRef = useRef<Gantt | null>(null);
  const navigate = useNavigateWithSearch();

  useEffect(() => {
    if (!svgRef.current || tasks.length === 0) {
      return;
    }

    ganttRef.current = new Gantt(svgRef.current, tasks, {
      view_mode: viewMode,
      on_click: (task: FrappeGanttTask) => {
        navigate(paths.task(projectId, task.id));
      },
      readonly: true,
    });

    return () => {
      ganttRef.current?.clear();
      ganttRef.current = null;
    };
  }, [tasks, viewMode, projectId, navigate]);

  if (tasks.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        No tasks to display
      </div>
    );
  }

  return (
    <div className="gantt-container w-full h-full overflow-auto">
      <svg ref={svgRef} />
    </div>
  );
}
