import { Fragment, useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, RefreshCw, ArrowLeft, Loader2 } from 'lucide-react';

import { useProject } from '@/contexts/ProjectContext';
import { ClickedElementsProvider } from '@/contexts/ClickedElementsProvider';
import { ReviewProvider } from '@/contexts/ReviewProvider';
import {
  GitOperationsProvider,
  useGitOperationsError,
} from '@/contexts/GitOperationsContext';
import { ExecutionProcessesProvider } from '@/contexts/ExecutionProcessesContext';
import { useGanttTasks } from '@/hooks/useGanttTasks';
import {
  useBranchStatus,
  useTaskDetailNavigation,
  useTaskPanelHeader,
} from '@/hooks';
import { GanttChart } from '@/components/gantt/GanttChart';
import { GanttToolbar } from '@/components/gantt/GanttToolbar';
import { TasksLayout } from '@/components/layout/TasksLayout';
import TaskAttemptPanel from '@/components/panels/TaskAttemptPanel';
import { DiffsPanelContainer } from '@/components/panels/DiffsPanelContainer';
import { PreviewPanel } from '@/components/panels/PreviewPanel';
import TaskPanel from '@/components/panels/TaskPanel';
import TodoPanel from '@/components/tasks/TodoPanel';
import { NewCard, NewCardHeader } from '@/components/ui/new-card';
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

function GitErrorBanner() {
  const { error: gitError } = useGitOperationsError();

  if (!gitError) return null;

  return (
    <div className="mx-4 mt-4 p-3 border border-destructive rounded">
      <div className="text-destructive text-sm">{gitError}</div>
    </div>
  );
}

export function GanttView() {
  const { t } = useTranslation(['tasks', 'common']);
  const navigate = useNavigate();

  const [colorMode, setColorMode] = useState<'status' | 'group'>('status');

  const {
    projectId,
    project,
    isLoading: projectLoading,
    error: projectError,
  } = useProject();

  const navigation = useTaskDetailNavigation({
    projectId,
    basePath: 'gantt',
  });
  const header = useTaskPanelHeader(navigation);
  const { data: branchStatus } = useBranchStatus(navigation.attempt?.id);
  const { openTask } = navigation;

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

  const handleRetry = useCallback(() => {
    window.location.reload();
  }, []);

  const handleBackToTasks = useCallback(() => {
    if (projectId) {
      navigate(paths.projectTasks(projectId));
    }
  }, [projectId, navigate]);

  const handleSelectTask = useCallback(
    (taskId: string) => {
      openTask(taskId);
    },
    [openTask]
  );

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

  const rightHeader = navigation.task ? (
    <NewCardHeader className="shrink-0" actions={header.headerActions}>
      <div className="mx-auto w-full">
        <Breadcrumb>
          <BreadcrumbList>
            {header.breadcrumbs.map((crumb, index) => (
              <Fragment key={`${crumb.label}-${index}`}>
                <BreadcrumbItem>
                  {crumb.isLink ? (
                    <BreadcrumbLink
                      className="cursor-pointer hover:underline"
                      onClick={crumb.onClick}
                    >
                      {crumb.label}
                    </BreadcrumbLink>
                  ) : (
                    <BreadcrumbPage>{crumb.label}</BreadcrumbPage>
                  )}
                </BreadcrumbItem>
                {index < header.breadcrumbs.length - 1 && (
                  <BreadcrumbSeparator />
                )}
              </Fragment>
            ))}
          </BreadcrumbList>
        </Breadcrumb>
      </div>
    </NewCardHeader>
  ) : null;

  const attemptContent = navigation.task ? (
    <NewCard className="h-full min-h-0 flex flex-col bg-diagonal-lines bg-muted border-0">
      {navigation.isTaskView ? (
        <TaskPanel task={navigation.task} basePath="gantt" />
      ) : (
        <TaskAttemptPanel
          attempt={navigation.attempt ?? undefined}
          task={navigation.task}
        >
          {({ logs, followUp }) => (
            <>
              <GitErrorBanner />
              <div className="flex-1 min-h-0 flex flex-col">
                <div className="flex-1 min-h-0 flex flex-col">{logs}</div>

                <div className="shrink-0 border-t">
                  <div className="mx-auto w-full max-w-[50rem]">
                    <TodoPanel />
                  </div>
                </div>

                <div className="min-h-0 max-h-[50%] border-t overflow-hidden bg-background">
                  <div className="mx-auto w-full max-w-[50rem] h-full min-h-0">
                    {followUp}
                  </div>
                </div>
              </div>
            </>
          )}
        </TaskAttemptPanel>
      )}
    </NewCard>
  ) : null;

  const auxContent =
    navigation.task && navigation.attempt ? (
      <div className="relative h-full w-full">
        {navigation.mode === 'preview' && <PreviewPanel />}
        {navigation.mode === 'diffs' && (
          <DiffsPanelContainer
            attempt={navigation.attempt}
            selectedTask={navigation.task}
            branchStatus={branchStatus ?? null}
          />
        )}
      </div>
    ) : (
      <div className="relative h-full w-full" />
    );

  const attemptArea = (
    <GitOperationsProvider attemptId={navigation.attempt?.id}>
      <ClickedElementsProvider attempt={navigation.attempt ?? null}>
        <ReviewProvider attemptId={navigation.attempt?.id}>
          <ExecutionProcessesProvider attemptId={navigation.attempt?.id}>
            <TasksLayout
              kanban={ganttContent}
              attempt={attemptContent}
              aux={auxContent}
              isPanelOpen={navigation.isPanelOpen}
              mode={navigation.mode ?? null}
              rightHeader={rightHeader}
            />
          </ExecutionProcessesProvider>
        </ReviewProvider>
      </ClickedElementsProvider>
    </GitOperationsProvider>
  );

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

      <div className="flex-1 min-h-0">{attemptArea}</div>
    </div>
  );
}
