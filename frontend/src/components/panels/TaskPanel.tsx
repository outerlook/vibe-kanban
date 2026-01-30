import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { useProject } from '@/contexts/ProjectContext';
import { useTaskAttemptsStream } from '@/hooks/useTaskAttemptsStream';
import { useNavigateWithSearch } from '@/hooks';
import { paths } from '@/lib/paths';
import { feedbackApi } from '@/lib/api';
import type { TaskWithAttemptStatus, WorkspaceWithSession } from 'shared/types';
import { NewCardContent } from '../ui/new-card';
import { Button } from '../ui/button';
import { PlusIcon, MessageSquare } from 'lucide-react';
import { HookStatusDetails } from '@/components/tasks/HookStatusDetails';
import { useProjectTasksContext } from '@/contexts/ProjectTasksContext';
import { CreateAttemptDialog } from '@/components/dialogs/tasks/CreateAttemptDialog';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { DataTable, type ColumnDef } from '@/components/ui/table';
import { formatRelativeTime } from '@/lib/utils';

interface TaskPanelProps {
  task: TaskWithAttemptStatus | null;
}

const TaskPanel = ({ task }: TaskPanelProps) => {
  const { t } = useTranslation('tasks');
  const navigate = useNavigateWithSearch();
  const { projectId } = useProject();
  const { hookExecutionsByTaskId } = useProjectTasksContext();

  // Stream workspaces with sessions via WebSocket
  const {
    attempts: attemptsWithSessions,
    attemptsById,
    isLoading: isAttemptsLoading,
    error: streamError,
  } = useTaskAttemptsStream(task?.id);
  const isAttemptsError = !!streamError;

  const parentAttempt = task?.parent_workspace_id
    ? attemptsById[task.parent_workspace_id]
    : undefined;
  const isParentLoading = isAttemptsLoading && !!task?.parent_workspace_id;

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
      accessor: (attempt) => formatRelativeTime(attempt.created_at),
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

            {hookExecutionsByTaskId[task.id]?.length > 0 && (
              <div className="rounded-lg border bg-card">
                <div className="px-4 py-2 border-b text-sm font-medium">
                  Hooks
                </div>
                <div className="px-4">
                  <HookStatusDetails taskId={task.id} />
                </div>
              </div>
            )}
          </div>
        </div>
      </NewCardContent>
    </>
  );
};

export default TaskPanel;
