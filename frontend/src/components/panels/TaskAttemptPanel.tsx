import type { TaskWithAttemptStatus, WorkspaceWithSession } from 'shared/types';
import VirtualizedList from '@/components/logs/VirtualizedList';
import { TaskFollowUpSection } from '@/components/tasks/TaskFollowUpSection';
import { FeedbackSection } from '@/components/feedback/FeedbackSection';
import { EntriesProvider } from '@/contexts/EntriesContext';
import { RetryUiProvider } from '@/contexts/RetryUiContext';
import type { ReactNode } from 'react';

interface TaskAttemptPanelProps {
  attempt: WorkspaceWithSession | undefined;
  task: TaskWithAttemptStatus | null;
  children: (sections: {
    logs: ReactNode;
    followUp: ReactNode;
    feedback: ReactNode;
  }) => ReactNode;
}

const TaskAttemptPanel = ({
  attempt,
  task,
  children,
}: TaskAttemptPanelProps) => {
  if (!attempt) {
    return <div className="p-6 text-muted-foreground">Loading attempt...</div>;
  }

  if (!task) {
    return <div className="p-6 text-muted-foreground">Loading task...</div>;
  }

  return (
    <EntriesProvider key={attempt.id}>
      <RetryUiProvider attemptId={attempt.id}>
        {children({
          logs: (
            <VirtualizedList
              key={attempt.id}
              mode={{ type: 'workspace', attempt, task }}
            />
          ),
          followUp: (
            <TaskFollowUpSection task={task} session={attempt.session} />
          ),
          feedback: <FeedbackSection workspaceId={attempt.id} />,
        })}
      </RetryUiProvider>
    </EntriesProvider>
  );
};

export default TaskAttemptPanel;
