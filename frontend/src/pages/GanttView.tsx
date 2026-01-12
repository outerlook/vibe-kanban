import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, RefreshCw, ArrowLeft, Loader2 } from 'lucide-react';

import { useProject } from '@/contexts/ProjectContext';
import { useGanttTasks } from '@/hooks/useGanttTasks';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { GanttChart } from '@/components/gantt/GanttChart';
import { GanttToolbar } from '@/components/gantt/GanttToolbar';
import { TasksLayout } from '@/components/layout/TasksLayout';
import TaskPanel from '@/components/panels/TaskPanel';
import { TaskPanelHeaderActions } from '@/components/panels/TaskPanelHeaderActions';
import { NewCardHeader } from '@/components/ui/new-card';
import { Loader } from '@/components/ui/loader';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb';
import { paths } from '@/lib/paths';

export function GanttView() {
  const { t } = useTranslation(['tasks', 'common']);
  const navigate = useNavigate();

  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [isPanelOpen, setIsPanelOpen] = useState(false);
  const [colorMode, setColorMode] = useState<'status' | 'group'>('status');

  const {
    projectId,
    project,
    isLoading: projectLoading,
    error: projectError,
  } = useProject();

  const {
    ganttTasks,
    ganttLinks,
    isLoading: ganttLoading,
    isLoadingMore,
    hasMore,
    total,
    loadMore,
    error: ganttError,
  } = useGanttTasks(projectId, { colorMode });

  const { tasksById } = useProjectTasks(projectId ?? '');
  const selectedTask = selectedTaskId ? tasksById[selectedTaskId] ?? null : null;

  const handleRetry = useCallback(() => {
    window.location.reload();
  }, []);

  const handleBackToTasks = useCallback(() => {
    if (projectId) {
      navigate(paths.projectTasks(projectId));
    }
  }, [projectId, navigate]);

  const handleSelectTask = useCallback((taskId: string) => {
    setSelectedTaskId(taskId);
    setIsPanelOpen(true);
  }, []);

  const handleClosePanel = useCallback(() => {
    setIsPanelOpen(false);
  }, []);

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

  const ganttContent = (
    <div className="h-full flex flex-col">
      {total > 0 && hasMore && (
        <div className="flex flex-col items-center gap-2 py-4 border-b">
          <Button
            onClick={loadMore}
            disabled={isLoadingMore}
            variant="secondary"
          >
            {isLoadingMore && (
              <Loader2 className="h-4 w-4 animate-spin mr-2" />
            )}
            {t('pagination.loadMore', { defaultValue: 'Load more' })}
          </Button>
          <div className="text-xs text-muted-foreground">
            {t('pagination.showing', {
              defaultValue: 'Showing {{count}} of {{total}} tasks',
              count: ganttTasks.length,
              total,
            })}
          </div>
        </div>
      )}
      <div className="flex-1 min-h-0">
        <GanttChart
          tasks={ganttTasks}
          links={ganttLinks}
          onSelectTask={handleSelectTask}
        />
      </div>
    </div>
  );

  const rightHeader = selectedTask ? (
    <NewCardHeader
      className="shrink-0"
      actions={
        <TaskPanelHeaderActions
          task={selectedTask}
          onClose={handleClosePanel}
        />
      }
    />
  ) : null;

  return (
    <div className="h-full flex flex-col">
      <div className="shrink-0 border-b px-4 py-3 flex items-center justify-between">
        <div className="flex items-center">
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
        <GanttToolbar colorMode={colorMode} onColorModeChange={setColorMode} />
      </div>

      <div className="flex-1 min-h-0">
        <TasksLayout
          kanban={ganttContent}
          attempt={<TaskPanel task={selectedTask} />}
          aux={null}
          isPanelOpen={isPanelOpen}
          mode={null}
          rightHeader={rightHeader}
        />
      </div>
    </div>
  );
}
