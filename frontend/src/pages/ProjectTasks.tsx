import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { AlertTriangle, Plus, X } from 'lucide-react';
import { Loader } from '@/components/ui/loader';
import { openTaskForm } from '@/lib/openTaskForm';
import { FeatureShowcaseDialog } from '@/components/dialogs/global/FeatureShowcaseDialog';
import { showcases } from '@/config/showcases';
import { useUserSystem } from '@/components/ConfigProvider';
import { usePostHog } from 'posthog-js/react';

import { useSearch } from '@/contexts/SearchContext';
import { useTaskFilters } from '@/hooks/useTaskFilters';
import { useFilteredTasks } from '@/hooks/useFilteredTasks';
import { TaskFilterBar } from '@/components/tasks/TaskFilterBar';
import { useProject } from '@/contexts/ProjectContext';
import { useTaskAttemptsStream } from '@/hooks/useTaskAttemptsStream';
import { useTaskAttemptWithSession } from '@/hooks/useTaskAttempt';
import { ConnectionStatusBanner } from '@/components/common/ConnectionStatusBanner';
import { useTask } from '@/hooks/useTask';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { BranchStatusProvider } from '@/contexts/BranchStatusContext';
import { paths } from '@/lib/paths';
import { ExecutionProcessesProvider } from '@/contexts/ExecutionProcessesContext';
import { ClickedElementsProvider } from '@/contexts/ClickedElementsProvider';
import { ReviewProvider } from '@/contexts/ReviewProvider';
import { GitOperationsProvider } from '@/contexts/GitOperationsContext';
import {
  useKeyCreate,
  useKeyExit,
  useKeyFocusSearch,
  useKeyNavUp,
  useKeyNavDown,
  useKeyNavLeft,
  useKeyNavRight,
  useKeyOpenDetails,
  Scope,
  useKeyDeleteTask,
  useKeyCycleViewBackward,
} from '@/keyboard';

import TaskKanbanBoard from '@/components/tasks/TaskKanbanBoard';
import type { DragEndEvent } from '@/components/ui/shadcn-io/kanban';
import {
  useProjectTasks,
  type SharedTaskRecord,
} from '@/hooks/useProjectTasks';
import { useTaskMutations } from '@/hooks/useTaskMutations';
import { GroupView } from '@/components/tasks/GroupView';
import { ViewToggle, type ViewMode } from '@/components/tasks/ViewToggle';
import { useUnreadSync } from '@/hooks/useUnreadSync';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { useHotkeysContext } from 'react-hotkeys-hook';
import { TasksLayout, type LayoutMode } from '@/components/layout/TasksLayout';
import { PreviewPanel } from '@/components/panels/PreviewPanel';
import TaskAttemptPanel from '@/components/panels/TaskAttemptPanel';
import TaskPanel from '@/components/panels/TaskPanel';
import SharedTaskPanel from '@/components/panels/SharedTaskPanel';
import TodoPanel from '@/components/tasks/TodoPanel';
import { useAuth } from '@/hooks';
import { NewCard, NewCardHeader } from '@/components/ui/new-card';
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb';
import { AttemptHeaderActions } from '@/components/panels/AttemptHeaderActions';
import { TaskPanelHeaderActions } from '@/components/panels/TaskPanelHeaderActions';
import {
  DiffsPanelContainer,
  GitErrorBanner,
} from '@/components/panels/AttemptPanels';
import { TaskSelectionProvider } from '@/contexts/TaskSelectionContext';
import { TaskGroupsProvider } from '@/contexts/TaskGroupsContext';
import { BulkActionsBar } from '@/components/tasks/BulkActionsBar';

import type { TaskWithAttemptStatus } from 'shared/types';
import { TASK_STATUSES, normalizeStatus } from '@/constants/taskStatuses';

type Task = TaskWithAttemptStatus;

export function ProjectTasks() {
  const { t } = useTranslation(['tasks', 'common']);
  const { taskId, attemptId } = useParams<{
    projectId: string;
    taskId?: string;
    attemptId?: string;
  }>();
  const navigate = useNavigate();
  const { enableScope, disableScope, activeScopes } = useHotkeysContext();
  const [searchParams, setSearchParams] = useSearchParams();
  const isXL = useMediaQuery('(min-width: 1280px)');
  const isMobile = !isXL;
  const posthog = usePostHog();
  const [selectedSharedTaskId, setSelectedSharedTaskId] = useState<
    string | null
  >(null);
  const { userId } = useAuth();

  const {
    projectId,
    isLoading: projectLoading,
    error: projectError,
  } = useProject();

  useEffect(() => {
    enableScope(Scope.KANBAN);

    return () => {
      disableScope(Scope.KANBAN);
    };
  }, [enableScope, disableScope]);

  const handleCreateTask = useCallback(() => {
    if (projectId) {
      openTaskForm({ mode: 'create', projectId });
    }
  }, [projectId]);
  const { focusInput } = useSearch();
  const { filters } = useTaskFilters();

  const {
    tasks,
    tasksById,
    sharedTasksById,
    sharedOnlyByStatus,
    isLoading,
    isInitialSyncComplete,
    loadMoreByStatus,
    isLoadingMoreByStatus,
    hasMoreByStatus,
    totalByStatus,
    error: tasksError,
  } = useProjectTasks(projectId || '');

  const { updateTask } = useTaskMutations(projectId || undefined);

  // Sync unread state for this project
  useUnreadSync({ projectId: projectId || '', tasks });

  const {
    data: selectedTaskFallback,
    isLoading: isTaskFallbackLoading,
    isSuccess: isTaskFallbackSuccess,
    isError: isTaskFallbackError,
  } = useTask(taskId, {
    enabled: !!taskId && !tasksById[taskId],
  });

  const selectedTask = useMemo(
    () =>
      taskId ? (tasksById[taskId] ?? selectedTaskFallback ?? null) : null,
    [taskId, tasksById, selectedTaskFallback]
  );

  const selectedSharedTask = useMemo(() => {
    if (!selectedSharedTaskId) return null;
    return sharedTasksById[selectedSharedTaskId] ?? null;
  }, [selectedSharedTaskId, sharedTasksById]);

  useEffect(() => {
    if (taskId) {
      setSelectedSharedTaskId(null);
    }
  }, [taskId]);

  const isTaskPanelOpen = Boolean(taskId && selectedTask);
  const isSharedPanelOpen = Boolean(selectedSharedTask);
  const isPanelOpen = isTaskPanelOpen || isSharedPanelOpen;

  const { config, updateAndSaveConfig, loading } = useUserSystem();

  const isLoaded = !loading;
  const showcaseId = showcases.taskPanel.id;
  const seenFeatures = useMemo(
    () => config?.showcases?.seen_features ?? [],
    [config?.showcases?.seen_features]
  );
  const seen = isLoaded && seenFeatures.includes(showcaseId);

  useEffect(() => {
    if (!isLoaded || !isPanelOpen || seen) return;

    FeatureShowcaseDialog.show({ config: showcases.taskPanel }).finally(() => {
      FeatureShowcaseDialog.hide();
      if (seenFeatures.includes(showcaseId)) return;
      void updateAndSaveConfig({
        showcases: { seen_features: [...seenFeatures, showcaseId] },
      });
    });
  }, [
    isLoaded,
    isPanelOpen,
    seen,
    showcaseId,
    updateAndSaveConfig,
    seenFeatures,
  ]);

  const isLatest = attemptId === 'latest';
  const {
    attempts,
    isConnected: isAttemptsConnected,
    isLoading: isAttemptsLoading,
    error: attemptsError,
  } = useTaskAttemptsStream(isLatest ? taskId : undefined);

  // Latest attempt is attempts[0] (already sorted by created_at DESC)
  const latestAttemptId = useMemo(() => {
    if (!attempts.length) return undefined;
    return attempts[0].id;
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
      navigateWithSearch(paths.task(projectId, taskId), { replace: true });
      return;
    }

    navigateWithSearch(paths.attempt(projectId, taskId, latestAttemptId), {
      replace: true,
    });
  }, [
    projectId,
    taskId,
    isLatest,
    isAttemptsLoading,
    latestAttemptId,
    navigate,
    navigateWithSearch,
  ]);

  // Redirect to task list if task doesn't exist after all data has loaded
  useEffect(() => {
    if (!projectId || !taskId || !isInitialSyncComplete) return;

    // Check if we need to wait for the fallback query
    const taskInLocalState = !!tasksById[taskId];
    const needsFallback = !taskInLocalState;

    // If we need the fallback, wait until it has completed (success or error)
    // Note: isLoading is false for disabled queries, so we check completion status
    if (needsFallback) {
      const fallbackComplete = isTaskFallbackSuccess || isTaskFallbackError;
      if (!fallbackComplete && isTaskFallbackLoading) return;
      // Even if not loading, if it hasn't completed (success/error), it means
      // the query was just enabled and hasn't fetched yet - wait
      if (!fallbackComplete) return;
    }

    if (selectedTask === null) {
      navigate(`/projects/${projectId}/tasks`, { replace: true });
    }
  }, [
    projectId,
    taskId,
    isInitialSyncComplete,
    isTaskFallbackLoading,
    isTaskFallbackSuccess,
    isTaskFallbackError,
    selectedTask,
    tasksById,
    navigate,
  ]);

  const effectiveAttemptId = attemptId === 'latest' ? undefined : attemptId;
  const isTaskView = !!taskId && !effectiveAttemptId;
  const { data: attempt } = useTaskAttemptWithSession(effectiveAttemptId);

  const rawMode = searchParams.get('view') as LayoutMode;
  const mode: LayoutMode =
    rawMode === 'preview' || rawMode === 'diffs' ? rawMode : null;

  // TODO: Remove this redirect after v0.1.0 (legacy URL support for bookmarked links)
  // Migrates old `view=logs` to `view=diffs`
  useEffect(() => {
    const view = searchParams.get('view');
    if (view === 'logs') {
      const params = new URLSearchParams(searchParams);
      params.set('view', 'diffs');
      setSearchParams(params, { replace: true });
    }
  }, [searchParams, setSearchParams]);

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

  // View mode: 'kanban' or 'groups'
  const rawViewMode = searchParams.get('view_mode') as ViewMode | null;
  const viewMode: ViewMode = rawViewMode === 'groups' ? 'groups' : 'kanban';

  const setViewMode = useCallback(
    (newViewMode: ViewMode) => {
      const params = new URLSearchParams(searchParams);
      if (newViewMode === 'kanban') {
        params.delete('view_mode');
      } else {
        params.set('view_mode', newViewMode);
      }
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams]
  );

  const handleGroupClick = useCallback(
    (groupId: string) => {
      const params = new URLSearchParams(searchParams);
      params.set('group', groupId);
      params.delete('view_mode');
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams]
  );

  const handleCreateNewTask = useCallback(() => {
    handleCreateTask();
  }, [handleCreateTask]);

  useKeyCreate(handleCreateNewTask, {
    scope: Scope.KANBAN,
    preventDefault: true,
  });

  useKeyFocusSearch(
    () => {
      focusInput();
    },
    {
      scope: Scope.KANBAN,
      preventDefault: true,
    }
  );

  useKeyExit(
    () => {
      if (isPanelOpen) {
        handleClosePanel();
      } else {
        navigate('/projects');
      }
    },
    { scope: Scope.KANBAN }
  );

  const showSharedTasks = searchParams.get('shared') !== 'off';

  useEffect(() => {
    if (showSharedTasks) return;
    if (!selectedSharedTaskId) return;
    const sharedTask = sharedTasksById[selectedSharedTaskId];
    if (sharedTask && sharedTask.assignee_user_id === userId) {
      return;
    }
    setSelectedSharedTaskId(null);
  }, [selectedSharedTaskId, sharedTasksById, showSharedTasks, userId]);

  const {
    kanbanColumns,
    visibleTasksByStatus,
    hasVisibleLocalTasks,
    hasVisibleSharedTasks,
  } = useFilteredTasks({
    tasks,
    sharedTasksById,
    sharedOnlyByStatus,
    filters,
    showSharedTasks,
    userId,
  });

  useKeyNavUp(
    () => {
      selectPreviousTask();
    },
    {
      scope: Scope.KANBAN,
      preventDefault: true,
    }
  );

  useKeyNavDown(
    () => {
      selectNextTask();
    },
    {
      scope: Scope.KANBAN,
      preventDefault: true,
    }
  );

  useKeyNavLeft(
    () => {
      selectPreviousColumn();
    },
    {
      scope: Scope.KANBAN,
      preventDefault: true,
    }
  );

  useKeyNavRight(
    () => {
      selectNextColumn();
    },
    {
      scope: Scope.KANBAN,
      preventDefault: true,
    }
  );

  /**
   * Cycle the attempt area view.
   * - When panel is closed: opens task details (if a task is selected)
   * - When panel is open: cycles among [attempt, preview, diffs]
   */
  const cycleView = useCallback(
    (direction: 'forward' | 'backward' = 'forward') => {
      const order: LayoutMode[] = [null, 'preview', 'diffs'];
      const idx = order.indexOf(mode);
      const next =
        direction === 'forward'
          ? order[(idx + 1) % order.length]
          : order[(idx - 1 + order.length) % order.length];
      setMode(next);
    },
    [mode, setMode]
  );

  const cycleViewForward = useCallback(() => cycleView('forward'), [cycleView]);
  const cycleViewBackward = useCallback(
    () => cycleView('backward'),
    [cycleView]
  );

  // meta/ctrl+enter → open details or cycle forward
  const isFollowUpReadyActive = activeScopes.includes(Scope.FOLLOW_UP_READY);

  useKeyOpenDetails(
    () => {
      if (isPanelOpen) {
        // Track keyboard shortcut before cycling view
        const order: LayoutMode[] = [null, 'preview', 'diffs'];
        const idx = order.indexOf(mode);
        const next = order[(idx + 1) % order.length];

        if (next === 'preview') {
          posthog?.capture('preview_navigated', {
            trigger: 'keyboard',
            direction: 'forward',
            timestamp: new Date().toISOString(),
            source: 'frontend',
          });
        } else if (next === 'diffs') {
          posthog?.capture('diffs_navigated', {
            trigger: 'keyboard',
            direction: 'forward',
            timestamp: new Date().toISOString(),
            source: 'frontend',
          });
        }

        cycleViewForward();
      } else if (selectedTask) {
        handleViewTaskDetails(selectedTask);
      }
    },
    { scope: Scope.KANBAN, when: () => !isFollowUpReadyActive }
  );

  // meta/ctrl+shift+enter → cycle backward
  useKeyCycleViewBackward(
    () => {
      if (isPanelOpen) {
        // Track keyboard shortcut before cycling view
        const order: LayoutMode[] = [null, 'preview', 'diffs'];
        const idx = order.indexOf(mode);
        const next = order[(idx - 1 + order.length) % order.length];

        if (next === 'preview') {
          posthog?.capture('preview_navigated', {
            trigger: 'keyboard',
            direction: 'backward',
            timestamp: new Date().toISOString(),
            source: 'frontend',
          });
        } else if (next === 'diffs') {
          posthog?.capture('diffs_navigated', {
            trigger: 'keyboard',
            direction: 'backward',
            timestamp: new Date().toISOString(),
            source: 'frontend',
          });
        }

        cycleViewBackward();
      }
    },
    { scope: Scope.KANBAN, preventDefault: true }
  );

  useKeyDeleteTask(
    () => {
      // Note: Delete is now handled by TaskActionsDropdown
      // This keyboard shortcut could trigger the dropdown action if needed
    },
    {
      scope: Scope.KANBAN,
      preventDefault: true,
    }
  );

  const handleClosePanel = useCallback(() => {
    if (projectId) {
      navigate(`/projects/${projectId}/tasks`, { replace: true });
    }
  }, [projectId, navigate]);

  const handleViewTaskDetails = useCallback(
    (task: Task, attemptIdToShow?: string) => {
      if (!projectId) return;
      setSelectedSharedTaskId(null);

      if (attemptIdToShow) {
        navigateWithSearch(paths.attempt(projectId, task.id, attemptIdToShow));
      } else {
        navigateWithSearch(`${paths.task(projectId, task.id)}/attempts/latest`);
      }
    },
    [projectId, navigateWithSearch]
  );

  const handleViewSharedTask = useCallback(
    (sharedTask: SharedTaskRecord) => {
      setSelectedSharedTaskId(sharedTask.id);
      setMode(null);
      if (projectId) {
        navigateWithSearch(paths.projectTasks(projectId), { replace: true });
      }
    },
    [navigateWithSearch, projectId, setMode]
  );

  const selectNextTask = useCallback(() => {
    if (selectedTask) {
      const statusKey = normalizeStatus(selectedTask.status);
      const tasksInStatus = visibleTasksByStatus[statusKey] || [];
      const currentIndex = tasksInStatus.findIndex(
        (task) => task.id === selectedTask.id
      );
      if (currentIndex >= 0 && currentIndex < tasksInStatus.length - 1) {
        handleViewTaskDetails(tasksInStatus[currentIndex + 1]);
      }
    } else {
      for (const status of TASK_STATUSES) {
        const tasks = visibleTasksByStatus[status];
        if (tasks && tasks.length > 0) {
          handleViewTaskDetails(tasks[0]);
          break;
        }
      }
    }
  }, [selectedTask, visibleTasksByStatus, handleViewTaskDetails]);

  const selectPreviousTask = useCallback(() => {
    if (selectedTask) {
      const statusKey = normalizeStatus(selectedTask.status);
      const tasksInStatus = visibleTasksByStatus[statusKey] || [];
      const currentIndex = tasksInStatus.findIndex(
        (task) => task.id === selectedTask.id
      );
      if (currentIndex > 0) {
        handleViewTaskDetails(tasksInStatus[currentIndex - 1]);
      }
    } else {
      for (const status of TASK_STATUSES) {
        const tasks = visibleTasksByStatus[status];
        if (tasks && tasks.length > 0) {
          handleViewTaskDetails(tasks[0]);
          break;
        }
      }
    }
  }, [selectedTask, visibleTasksByStatus, handleViewTaskDetails]);

  const selectNextColumn = useCallback(() => {
    if (selectedTask) {
      const currentStatus = normalizeStatus(selectedTask.status);
      const currentIndex = TASK_STATUSES.findIndex(
        (status) => status === currentStatus
      );
      for (let i = currentIndex + 1; i < TASK_STATUSES.length; i++) {
        const tasks = visibleTasksByStatus[TASK_STATUSES[i]];
        if (tasks && tasks.length > 0) {
          handleViewTaskDetails(tasks[0]);
          return;
        }
      }
    } else {
      for (const status of TASK_STATUSES) {
        const tasks = visibleTasksByStatus[status];
        if (tasks && tasks.length > 0) {
          handleViewTaskDetails(tasks[0]);
          break;
        }
      }
    }
  }, [selectedTask, visibleTasksByStatus, handleViewTaskDetails]);

  const selectPreviousColumn = useCallback(() => {
    if (selectedTask) {
      const currentStatus = normalizeStatus(selectedTask.status);
      const currentIndex = TASK_STATUSES.findIndex(
        (status) => status === currentStatus
      );
      for (let i = currentIndex - 1; i >= 0; i--) {
        const tasks = visibleTasksByStatus[TASK_STATUSES[i]];
        if (tasks && tasks.length > 0) {
          handleViewTaskDetails(tasks[0]);
          return;
        }
      }
    } else {
      for (const status of TASK_STATUSES) {
        const tasks = visibleTasksByStatus[status];
        if (tasks && tasks.length > 0) {
          handleViewTaskDetails(tasks[0]);
          break;
        }
      }
    }
  }, [selectedTask, visibleTasksByStatus, handleViewTaskDetails]);

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event;
      if (!over || !active.data.current) return;

      const draggedTaskId = active.id as string;
      const newStatus = over.id as Task['status'];
      const task = tasksById[draggedTaskId];
      if (!task || task.status === newStatus) return;

      updateTask.mutate({
        taskId: draggedTaskId,
        data: {
          title: task.title,
          description: task.description,
          status: newStatus,
          parent_workspace_id: task.parent_workspace_id,
          image_ids: null,
          task_group_id: null,
        },
      });
    },
    [tasksById, updateTask]
  );

  const getSharedTask = useCallback(
    (task: Task | null | undefined) => {
      if (!task) return undefined;
      if (task.shared_task_id) {
        return sharedTasksById[task.shared_task_id];
      }
      return sharedTasksById[task.id];
    },
    [sharedTasksById]
  );

  const hasSharedTasks = useMemo(() => {
    return Object.values(kanbanColumns).some((items) =>
      items.some((item) => {
        if (item.type === 'shared') return true;
        return Boolean(item.sharedTask);
      })
    );
  }, [kanbanColumns]);

  const isInitialTasksLoad = isLoading && tasks.length === 0;

  if (projectError) {
    return (
      <div className="p-4">
        <Alert>
          <AlertTitle className="flex items-center gap-2">
            <AlertTriangle size="16" />
            {t('common:states.error')}
          </AlertTitle>
          <AlertDescription>
            {projectError.message || 'Failed to load project'}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  if (projectLoading && isInitialTasksLoad) {
    return <Loader message={t('loading')} size={32} className="py-8" />;
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

  const kanbanContent =
    viewMode === 'groups' ? (
      <div className="flex flex-col h-full">
        <div className="shrink-0 px-4 py-2 flex items-center justify-end">
          <ViewToggle value={viewMode} onChange={setViewMode} />
        </div>
        <div className="flex-1 min-h-0 overflow-auto px-4 pb-4">
          <GroupView projectId={projectId!} onGroupClick={handleGroupClick} />
        </div>
      </div>
    ) : tasks.length === 0 && !hasSharedTasks ? (
      <div className="flex flex-col h-full">
        <div className="shrink-0 px-4 py-2 flex items-center justify-end">
          <ViewToggle value={viewMode} onChange={setViewMode} />
        </div>
        <div className="max-w-7xl mx-auto mt-8">
          <Card>
            <CardContent className="text-center py-8">
              <p className="text-muted-foreground">{t('empty.noTasks')}</p>
              <Button className="mt-4" onClick={handleCreateNewTask}>
                <Plus className="h-4 w-4 mr-2" />
                {t('empty.createFirst')}
              </Button>
            </CardContent>
          </Card>
        </div>
      </div>
    ) : (
      <div className="flex flex-col h-full">
        <div className="shrink-0 px-4 flex items-center gap-2">
          <div className="flex-1">
            <TaskFilterBar />
          </div>
          <ViewToggle value={viewMode} onChange={setViewMode} />
        </div>
        {!hasVisibleLocalTasks && !hasVisibleSharedTasks ? (
          <div className="max-w-7xl mx-auto mt-8">
            <Card>
              <CardContent className="text-center py-8">
                <p className="text-muted-foreground">
                  {t('empty.noSearchResults')}
                </p>
              </CardContent>
            </Card>
          </div>
        ) : (
          <div className="flex-1 min-h-0 w-full overflow-x-auto overflow-y-auto overscroll-x-contain">
            <TaskKanbanBoard
              columns={kanbanColumns}
              onDragEnd={handleDragEnd}
              onViewTaskDetails={handleViewTaskDetails}
              onViewSharedTask={handleViewSharedTask}
              selectedTaskId={selectedTask?.id}
              selectedSharedTaskId={selectedSharedTaskId}
              onCreateTask={handleCreateNewTask}
              projectId={projectId!}
              loadMoreByStatus={loadMoreByStatus}
              hasMoreByStatus={hasMoreByStatus}
              isLoadingMoreByStatus={isLoadingMoreByStatus}
              totalByStatus={totalByStatus}
            />
          </div>
        )}
      </div>
    );

  const rightHeader = selectedTask ? (
    <NewCardHeader
      className="shrink-0"
      actions={
        isTaskView ? (
          <TaskPanelHeaderActions
            task={selectedTask}
            sharedTask={getSharedTask(selectedTask)}
            onClose={() =>
              navigate(`/projects/${projectId}/tasks`, { replace: true })
            }
          />
        ) : (
          <AttemptHeaderActions
            mode={mode}
            onModeChange={setMode}
            task={selectedTask}
            sharedTask={getSharedTask(selectedTask)}
            attempt={attempt ?? null}
            onClose={() =>
              navigate(`/projects/${projectId}/tasks`, { replace: true })
            }
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
                  onClick={() =>
                    navigateWithSearch(paths.task(projectId!, taskId!))
                  }
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
  ) : selectedSharedTask ? (
    <NewCardHeader
      className="shrink-0"
      actions={
        <Button
          variant="icon"
          aria-label={t('common:buttons.close', { defaultValue: 'Close' })}
          onClick={() => {
            setSelectedSharedTaskId(null);
            if (projectId) {
              navigateWithSearch(paths.projectTasks(projectId), {
                replace: true,
              });
            }
          }}
        >
          <X size={16} />
        </Button>
      }
    >
      <div className="mx-auto w-full">
        <Breadcrumb>
          <BreadcrumbList>
            <BreadcrumbItem>
              <BreadcrumbPage>
                {truncateTitle(selectedSharedTask?.title)}
              </BreadcrumbPage>
            </BreadcrumbItem>
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
              {isLatest && (
                <ConnectionStatusBanner
                  isConnected={isAttemptsConnected}
                  error={attemptsError}
                  className="mx-4 mt-2"
                />
              )}
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
  ) : selectedSharedTask ? (
    <NewCard className="h-full min-h-0 flex flex-col bg-diagonal-lines bg-muted border-0">
      <SharedTaskPanel task={selectedSharedTask} />
    </NewCard>
  ) : null;

  const auxContent =
    selectedTask && attempt ? (
      <div className="relative h-full w-full">
        {mode === 'preview' && <PreviewPanel />}
        {mode === 'diffs' && (
          <DiffsPanelContainer attempt={attempt} selectedTask={selectedTask} />
        )}
      </div>
    ) : (
      <div className="relative h-full w-full" />
    );

  const effectiveMode: LayoutMode = selectedSharedTask ? null : mode;

  const attemptArea = (
    <TaskSelectionProvider>
      <TaskGroupsProvider>
        <GitOperationsProvider attemptId={attempt?.id}>
          <BranchStatusProvider attemptId={attempt?.id}>
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
                    kanban={kanbanContent}
                    attempt={attemptContent}
                    aux={auxContent}
                    isPanelOpen={isPanelOpen}
                    mode={effectiveMode}
                    isMobile={isMobile}
                    rightHeader={rightHeader}
                  />
                </ExecutionProcessesProvider>
              </ReviewProvider>
            </ClickedElementsProvider>
          </BranchStatusProvider>
        </GitOperationsProvider>
        <BulkActionsBar />
      </TaskGroupsProvider>
    </TaskSelectionProvider>
  );

  return (
    <div className="min-h-full h-full flex flex-col">
      {tasksError && (
        <Alert className="w-full z-30 xl:sticky xl:top-0">
          <AlertTitle className="flex items-center gap-2">
            <AlertTriangle size="16" />
            {t('common:states.error')}
          </AlertTitle>
          <AlertDescription>{tasksError}</AlertDescription>
        </Alert>
      )}

      <div className="flex-1 min-h-0">{attemptArea}</div>
    </div>
  );
}
