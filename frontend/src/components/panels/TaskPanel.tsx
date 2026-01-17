import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useQueries, useQuery } from '@tanstack/react-query';
import { useProject } from '@/contexts/ProjectContext';
import { useTaskAttemptsStream } from '@/hooks/useTaskAttemptsStream';
import { useTaskAttemptWithSession } from '@/hooks/useTaskAttempt';
import { useNavigateWithSearch } from '@/hooks';
import { paths } from '@/lib/paths';
import { sessionsApi, feedbackApi } from '@/lib/api';
import type { TaskWithAttemptStatus, Session } from 'shared/types';
import type { WorkspaceWithSession } from '@/types/attempt';
import { createWorkspaceWithSession } from '@/types/attempt';
import { NewCardContent } from '../ui/new-card';
import { Button } from '../ui/button';
import { PlusIcon, MessageSquare } from 'lucide-react';
import { CreateAttemptDialog } from '@/components/dialogs/tasks/CreateAttemptDialog';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { DataTable, type ColumnDef } from '@/components/ui/table';

interface TaskPanelProps {
  task: TaskWithAttemptStatus | null;
}

const TaskPanel = ({ task }: TaskPanelProps) => {
  const { t } = useTranslation('tasks');
  const navigate = useNavigateWithSearch();
  const { projectId } = useProject();

  // Stream workspaces via WebSocket
  const {
    attempts: workspaces,
    isLoading: isStreamLoading,
    error: streamError,
  } = useTaskAttemptsStream(task?.id);

  // Fetch sessions for each workspace (one-time fetch, sessions rarely change)
  const sessionQueries = useQueries({
    queries: workspaces.map((workspace) => ({
      queryKey: ['session', 'byWorkspace', workspace.id],
      queryFn: () => sessionsApi.getByWorkspace(workspace.id),
      staleTime: Infinity, // Sessions rarely change
    })),
  });

  // Combine workspaces with their sessions
  const attemptsWithSessions: WorkspaceWithSession[] = useMemo(() => {
    const sessionsById: Record<string, Session | undefined> = {};
    sessionQueries.forEach((query, index) => {
      if (query.data) {
        sessionsById[workspaces[index].id] = query.data[0];
      }
    });
    return workspaces.map((workspace) =>
      createWorkspaceWithSession(workspace, sessionsById[workspace.id])
    );
  }, [workspaces, sessionQueries]);

  const isAttemptsLoading =
    isStreamLoading || sessionQueries.some((q) => q.isLoading);
  const isAttemptsError = !!streamError;

  const { data: parentAttempt, isLoading: isParentLoading } =
    useTaskAttemptWithSession(task?.parent_workspace_id || undefined);

  // Fetch feedback for the task to show indicators
  const { data: feedbackList = [] } = useQuery({
    queryKey: ['feedback', 'byTask', task?.id],
    queryFn: () => feedbackApi.getByTaskId(task!.id),
    enabled: !!task?.id,
    staleTime: 30000, // 30s
  });

  // Map feedback by workspace_id for quick lookup
  const feedbackByWorkspaceId = useMemo(() => {
    const map = new Map<string, boolean>();
    feedbackList.forEach((fb) => {
      map.set(fb.workspace_id, true);
    });
    return map;
  }, [feedbackList]);

  const formatTimeAgo = (iso: string) => {
    const d = new Date(iso);
    const diffMs = Date.now() - d.getTime();
    const absSec = Math.round(Math.abs(diffMs) / 1000);

    const rtf =
      typeof Intl !== 'undefined' &&
      typeof Intl.RelativeTimeFormat === 'function'
        ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
        : null;

    const to = (value: number, unit: Intl.RelativeTimeFormatUnit) =>
      rtf
        ? rtf.format(-value, unit)
        : `${value} ${unit}${value !== 1 ? 's' : ''} ago`;

    if (absSec < 60) return to(Math.round(absSec), 'second');
    const mins = Math.round(absSec / 60);
    if (mins < 60) return to(mins, 'minute');
    const hours = Math.round(mins / 60);
    if (hours < 24) return to(hours, 'hour');
    const days = Math.round(hours / 24);
    if (days < 30) return to(days, 'day');
    const months = Math.round(days / 30);
    if (months < 12) return to(months, 'month');
    const years = Math.round(months / 12);
    return to(years, 'year');
  };

  // attemptsWithSessions already sorted by useTaskAttemptsStream
  const displayedAttempts = attemptsWithSessions;

  if (!task) {
    return (
      <div className="text-muted-foreground">
        {t('taskPanel.noTaskSelected')}
      </div>
    );
  }

  const titleContent = `# ${task.title || 'Task'}`;
  const descriptionContent = task.description || '';

  const attemptColumns: ColumnDef<WorkspaceWithSession>[] = [
    {
      id: 'executor',
      header: '',
      accessor: (attempt) => attempt.session?.executor || 'Base Agent',
      className: 'pr-4',
    },
    {
      id: 'branch',
      header: '',
      accessor: (attempt) => attempt.branch || '—',
      className: 'pr-4',
    },
    {
      id: 'feedback',
      header: '',
      accessor: (attempt) =>
        feedbackByWorkspaceId.has(attempt.id) ? (
          <MessageSquare
            size={14}
            className="text-muted-foreground"
            aria-label={t('taskPanel.hasFeedback')}
          />
        ) : null,
      className: 'w-6 pr-2',
    },
    {
      id: 'time',
      header: '',
      accessor: (attempt) => formatTimeAgo(attempt.created_at),
      className: 'pr-0 text-right',
    },
  ];

  return (
    <>
      <NewCardContent>
        <div className="p-6 flex flex-col h-full max-h-[calc(100vh-8rem)]">
          <div className="space-y-3 overflow-y-auto flex-shrink min-h-0">
            <WYSIWYGEditor value={titleContent} disabled />
            {descriptionContent && (
              <WYSIWYGEditor value={descriptionContent} disabled />
            )}
          </div>

          <div className="mt-6 flex-shrink-0 space-y-4">
            {task.parent_workspace_id && (
              <DataTable
                data={parentAttempt ? [parentAttempt] : []}
                columns={attemptColumns}
                keyExtractor={(attempt) => attempt.id}
                onRowClick={(attempt) => {
                  if (projectId) {
                    navigate(
                      paths.attempt(projectId, attempt.task_id, attempt.id)
                    );
                  }
                }}
                isLoading={isParentLoading}
                headerContent="Parent Attempt"
              />
            )}

            {isAttemptsLoading ? (
              <div className="text-muted-foreground">
                {t('taskPanel.loadingAttempts')}
              </div>
            ) : isAttemptsError ? (
              <div className="text-destructive">
                {t('taskPanel.errorLoadingAttempts')}
              </div>
            ) : (
              <DataTable
                data={displayedAttempts}
                columns={attemptColumns}
                keyExtractor={(attempt) => attempt.id}
                onRowClick={(attempt) => {
                  if (projectId && task.id) {
                    navigate(paths.attempt(projectId, task.id, attempt.id));
                  }
                }}
                emptyState={t('taskPanel.noAttempts')}
                headerContent={
                  <div className="w-full flex text-left">
                    <span className="flex-1">
                      {t('taskPanel.attemptsCount', {
                        count: displayedAttempts.length,
                      })}
                    </span>
                    <span>
                      <Button
                        variant="icon"
                        onClick={() =>
                          CreateAttemptDialog.show({
                            taskId: task.id,
                          })
                        }
                      >
                        <PlusIcon size={16} />
                      </Button>
                    </span>
                  </div>
                }
              />
            )}
          </div>
        </div>
      </NewCardContent>
    </>
  );
};

export default TaskPanel;
