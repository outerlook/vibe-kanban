import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, RefreshCw, ArrowLeft } from 'lucide-react';

import { useProject } from '@/contexts/ProjectContext';
import { useGanttTasks } from '@/hooks/useGanttTasks';
import { GanttChart, type GanttViewMode } from '@/components/gantt/GanttChart';
import { Loader } from '@/components/ui/loader';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb';
import { paths } from '@/lib/paths';

const VIEW_MODES: { value: GanttViewMode; label: string }[] = [
  { value: 'Day', label: 'Day' },
  { value: 'Week', label: 'Week' },
  { value: 'Month', label: 'Month' },
];

export function GanttView() {
  const { t } = useTranslation(['tasks', 'common']);
  const navigate = useNavigate();
  const [viewMode, setViewMode] = useState<GanttViewMode>('Week');

  const {
    projectId,
    project,
    isLoading: projectLoading,
    error: projectError,
  } = useProject();

  const {
    ganttTasks,
    isLoading: ganttLoading,
    error: ganttError,
  } = useGanttTasks(projectId);

  const handleRetry = useCallback(() => {
    window.location.reload();
  }, []);

  const handleBackToTasks = useCallback(() => {
    if (projectId) {
      navigate(paths.projectTasks(projectId));
    }
  }, [projectId, navigate]);

  if (projectError) {
    return (
      <div className="p-4">
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('common:states.error')}</AlertTitle>
          <AlertDescription>
            {projectError.message || 'Failed to load project'}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  if (projectLoading || ganttLoading) {
    return <Loader message={t('loading')} size={32} className="py-8" />;
  }

  if (ganttError) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4 p-8">
        <Alert variant="destructive" className="max-w-md">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('common:states.error')}</AlertTitle>
          <AlertDescription>{ganttError}</AlertDescription>
        </Alert>
        <Button onClick={handleRetry} variant="outline">
          <RefreshCw className="h-4 w-4 mr-2" />
          {t('common:buttons.retry', { defaultValue: 'Retry' })}
        </Button>
      </div>
    );
  }

  if (!projectId) {
    return (
      <div className="p-4">
        <Alert>
          <AlertTitle>{t('common:states.error')}</AlertTitle>
          <AlertDescription>Project not found</AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      <div className="shrink-0 border-b px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Button
            variant="ghost"
            size="sm"
            onClick={handleBackToTasks}
            className="gap-2"
          >
            <ArrowLeft className="h-4 w-4" />
            {t('common:buttons.back', { defaultValue: 'Back' })}
          </Button>
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink
                  className="cursor-pointer hover:underline"
                  onClick={handleBackToTasks}
                >
                  {project?.name || 'Project'}
                </BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>Gantt</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
        </div>

        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">View:</span>
          <Select
            value={viewMode}
            onValueChange={(value) => setViewMode(value as GanttViewMode)}
          >
            <SelectTrigger className="w-[100px] h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {VIEW_MODES.map((mode) => (
                <SelectItem key={mode.value} value={mode.value}>
                  {mode.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      <div className="flex-1 min-h-0">
        <GanttChart
          projectId={projectId}
          tasks={ganttTasks}
          viewMode={viewMode}
        />
      </div>
    </div>
  );
}
