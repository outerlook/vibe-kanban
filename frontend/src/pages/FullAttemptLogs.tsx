// VS Code webview integration - install keyboard/clipboard bridge
import '@/vscode/bridge';

import { useParams } from 'react-router-dom';
import { AppWithStyleOverride } from '@/utils/StyleOverride';
import { WebviewContextMenu } from '@/vscode/ContextMenu';
import TaskAttemptPanel from '@/components/panels/TaskAttemptPanel';
import { useTaskAttemptWithSession } from '@/hooks/useTaskAttempt';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { useTask } from '@/hooks/useTask';
import { ExecutionProcessesProvider } from '@/contexts/ExecutionProcessesContext';
import { ReviewProvider } from '@/contexts/ReviewProvider';
import { ClickedElementsProvider } from '@/contexts/ClickedElementsProvider';

export function FullAttemptLogsPage() {
  const {
    projectId = '',
    taskId = '',
    attemptId = '',
  } = useParams<{
    projectId: string;
    taskId: string;
    attemptId: string;
  }>();

  const { data: attempt } = useTaskAttemptWithSession(attemptId);
  const { tasksById } = useProjectTasks(projectId);
  const { data: taskFallback } = useTask(taskId, {
    enabled: !!taskId && !tasksById[taskId],
  });
  const task = taskId ? (tasksById[taskId] ?? taskFallback ?? null) : null;

  return (
    <AppWithStyleOverride>
      <div className="h-screen flex flex-col bg-muted">
        <WebviewContextMenu />

        <main className="flex-1 min-h-0">
          {attempt ? (
            <ClickedElementsProvider attempt={attempt}>
              <ReviewProvider key={attempt.id}>
                <ExecutionProcessesProvider
                  key={attempt.id}
                  source={{ type: 'workspace', workspaceId: attempt.id }}
                >
                  <TaskAttemptPanel attempt={attempt} task={task}>
                    {({ logs, followUp, feedback }) => (
                      <div className="h-full min-h-0 flex flex-col">
                        <div className="flex-1 min-h-0 flex flex-col">
                          {logs}
                        </div>
                        <div className="shrink-0">{feedback}</div>
                        <div className="min-h-0 max-h-[50%] border-t overflow-hidden">
                          <div className="mx-auto w-full max-w-[50rem] h-full min-h-0">
                            {followUp}
                          </div>
                        </div>
                      </div>
                    )}
                  </TaskAttemptPanel>
                </ExecutionProcessesProvider>
              </ReviewProvider>
            </ClickedElementsProvider>
          ) : (
            <TaskAttemptPanel attempt={attempt} task={task}>
              {({ logs, followUp, feedback }) => (
                <div className="h-full min-h-0 flex flex-col">
                  <div className="flex-1 min-h-0 flex flex-col">{logs}</div>
                  <div className="shrink-0">{feedback}</div>
                  <div className="min-h-0 max-h-[50%] border-t overflow-hidden">
                    <div className="mx-auto w-full max-w-[50rem] h-full min-h-0">
                      {followUp}
                    </div>
                  </div>
                </div>
              )}
            </TaskAttemptPanel>
          )}
        </main>
      </div>
    </AppWithStyleOverride>
  );
}
