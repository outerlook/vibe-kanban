import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, RefreshCw, ArrowLeft, Loader2 } from 'lucide-react';

import { useProject } from '@/contexts/ProjectContext';
import { useGanttTasks } from '@/hooks/useGanttTasks';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { useTask } from '@/hooks/useTask';
import { useTaskAttempts } from '@/hooks/useTaskAttempts';
import { useTaskAttemptWithSession } from '@/hooks/useTaskAttempt';
import { useBranchStatus } from '@/hooks';
import { GanttChart } from '@/components/gantt/GanttChart';
import { GanttToolbar } from '@/components/gantt/GanttToolbar';
import { TasksLayout, type LayoutMode } from '@/components/layout/TasksLayout';
import TaskPanel from '@/components/panels/TaskPanel';
import TaskAttemptPanel from '@/components/panels/TaskAttemptPanel';
import { TaskPanelHeaderActions } from '@/components/panels/TaskPanelHeaderActions';
import { AttemptHeaderActions } from '@/components/panels/AttemptHeaderActions';
import { PreviewPanel } from '@/components/panels/PreviewPanel';
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
import { ClickedElementsProvider } from '@/contexts/ClickedElementsProvider';
import { ExecutionProcessesProvider } from '@/contexts/ExecutionProcessesContext';
import { ReviewProvider } from '@/contexts/ReviewProvider';
import { GitOperationsProvider } from '@/contexts/GitOperationsContext';
import {
  DiffsPanelContainer,
  GitErrorBanner,
} from '@/components/panels/AttemptPanels';

export function GanttView() {
  const { t } = useTranslation(['tasks', 'common']);
  const { taskId, attemptId } = useParams<{
    projectId: string;
    taskId?: string;
    attemptId?: string;
  }>();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
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

  const { tasksById, isLoading: isTasksLoading } = useProjectTasks(
    projectId ?? ''
  );

  const { data: selectedTaskFallback } = useTask(taskId, {
    enabled: !!taskId && !tasksById[taskId],
  });

  const selectedTask = useMemo(
    () =>
      taskId ? (tasksById[taskId] ?? selectedTaskFallback ?? null) : null,
    [taskId, tasksById, selectedTaskFallback]
  );

  const isLatest = attemptId === 'latest';
  const { data: attempts = [], isLoading: isAttemptsLoading } = useTaskAttempts(
    taskId,
    {
      enabled: !!taskId && isLatest,
    }
  );

  const latestAttemptId = useMemo(() => {
    if (!attempts?.length) return undefined;
    return [...attempts].sort((a, b) => {
      const diff =
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
      if (diff !== 0) return diff;
      return a.id.localeCompare(b.id);
    })[0].id;
  }, [attempts]);

  const navigateWithSearch = useCallback(
    (pathname: string, options?: { replace?: boolean }) => {
      const search = searchParams.toString();
      navigate({ pathname, search: search ? `?${search}` : '' }, options);
    },
    [navigate, searchParams]
  );

  useEffect(() => {
    if (!projectId || !taskId) return;
    if (!isLatest) return;
    if (isAttemptsLoading) return;

    if (!latestAttemptId) {
      navigateWithSearch(paths.ganttTask(projectId, taskId), { replace: true });
      return;
    }

    navigateWithSearch(paths.ganttAttempt(projectId, taskId, latestAttemptId), {
      replace: true,
    });
  }, [
    projectId,
    taskId,
    isLatest,
    isAttemptsLoading,
    latestAttemptId,
    navigateWithSearch,
  ]);

  useEffect(() => {
    if (!projectId || !taskId || isTasksLoading) return;
    if (selectedTask === null) {
      navigate(paths.projectGantt(projectId), { replace: true });
    }
  }, [projectId, taskId, isTasksLoading, selectedTask, navigate]);

  const effectiveAttemptId = attemptId === 'latest' ? undefined : attemptId;
  const isTaskView = !!taskId && !effectiveAttemptId;
  const { data: attempt } = useTaskAttemptWithSession(effectiveAttemptId);
  const { data: branchStatus } = useBranchStatus(attempt?.id);

  const rawMode = searchParams.get('view') as LayoutMode;
  const mode: LayoutMode =
    rawMode === 'preview' || rawMode === 'diffs' ? rawMode : null;

  const setMode = useCallback(
    (newMode: LayoutMode) => {
      const params = new URLSearchParams(searchParams);
      if (newMode === null) {
        params.delete('view');
      } else {
        params.set('view', newMode);
      }
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams]
  );

  const handleRetry = useCallback(() => {
    window.location.reload();
  }, []);

  const handleBackToTasks = useCallback(() => {
    if (projectId) {
      navigate(paths.projectTasks(projectId));
    }
  }, [projectId, navigate]);

  const handleSelectTask = useCallback(
    (nextTaskId: string) => {
      if (projectId) {
        navigateWithSearch(paths.ganttTask(projectId, nextTaskId));
      }
    },
    [projectId, navigateWithSearch]
  );

  const handleClosePanel = useCallback(() => {
    if (projectId) {
      navigate(paths.projectGantt(projectId), { replace: true });
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

  const truncateTitle = (title: string | undefined, maxLength = 20) => {
    if (!title) return 'Task';
    if (title.length <= maxLength) return title;

    const truncated = title.substring(0, maxLength);
    const lastSpace = truncated.lastIndexOf(' ');

    return lastSpace > 0
      ? `${truncated.substring(0, lastSpace)}...`
      : `${truncated}...`;
  };

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
        isTaskView ? (
          <TaskPanelHeaderActions
            task={selectedTask}
            onClose={handleClosePanel}
          />
        ) : (
          <AttemptHeaderActions
            mode={mode}
            onModeChange={setMode}
            task={selectedTask}
            attempt={attempt ?? null}
            onClose={handleClosePanel}
          />
        )
      }
    >
      <div className="mx-auto w-full">
        <Breadcrumb>
          <BreadcrumbList>
            <BreadcrumbItem>
              {isTaskView ? (
                <BreadcrumbPage>
                  {truncateTitle(selectedTask?.title)}
                </BreadcrumbPage>
              ) : (
                <BreadcrumbLink
                  className="cursor-pointer hover:underline"
                  onClick={() => {
                    if (taskId) {
                      navigateWithSearch(paths.ganttTask(projectId, taskId));
                    }
                  }}
                >
                  {truncateTitle(selectedTask?.title)}
                </BreadcrumbLink>
              )}
            </BreadcrumbItem>
            {!isTaskView && (
              <>
                <BreadcrumbSeparator />
                <BreadcrumbItem>
                  <BreadcrumbPage>
                    {attempt?.branch || 'Task Attempt'}
                  </BreadcrumbPage>
                </BreadcrumbItem>
              </>
            )}
          </BreadcrumbList>
        </Breadcrumb>
      </div>
    </NewCardHeader>
  ) : null;

  const attemptContent = selectedTask ? (
    <NewCard className="h-full min-h-0 flex flex-col bg-diagonal-lines bg-muted border-0">
      {isTaskView ? (
        <TaskPanel task={selectedTask} />
      ) : (
        <TaskAttemptPanel attempt={attempt} task={selectedTask}>
          {({ logs, followUp, feedback }) => (
            <>
              <GitErrorBanner />
              <div className="flex-1 min-h-0 flex flex-col">
                <div className="flex-1 min-h-0 flex flex-col">{logs}</div>

                <div className="shrink-0 border-t">
                  <div className="mx-auto w-full max-w-[50rem]">
                    <TodoPanel />
                  </div>
                </div>

                <div className="shrink-0">{feedback}</div>

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
    selectedTask && attempt ? (
      <div className="relative h-full w-full">
        {mode === 'preview' && <PreviewPanel />}
        {mode === 'diffs' && (
          <DiffsPanelContainer
            attempt={attempt}
            selectedTask={selectedTask}
            branchStatus={branchStatus ?? null}
          />
        )}
      </div>
    ) : (
      <div className="relative h-full w-full" />
    );

  const effectiveMode: LayoutMode = selectedTask
    ? isTaskView
      ? null
      : mode
    : null;

  const isPanelOpen = Boolean(taskId && selectedTask);

  const attemptArea = (
    <GitOperationsProvider attemptId={attempt?.id}>
      <ClickedElementsProvider attempt={attempt}>
        <ReviewProvider attemptId={attempt?.id}>
          <ExecutionProcessesProvider
            source={
              attempt?.id
                ? { type: 'workspace', workspaceId: attempt.id }
                : undefined
            }
          >
            <TasksLayout
              kanban={ganttContent}
              attempt={attemptContent}
              aux={auxContent}
              isPanelOpen={isPanelOpen}
              mode={effectiveMode}
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
