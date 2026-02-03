import { useState } from 'react';
import { User } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { BaseAgentCapability } from 'shared/types';
import type { WorkspaceWithSession } from 'shared/types';
import { useUserSystem } from '@/components/ConfigProvider';
import { useRetryUi } from '@/contexts/RetryUiContext';
import { useAttemptExecution } from '@/hooks/useAttemptExecution';
import { RetryEditorInline } from './RetryEditorInline';

const UserMessage = ({
  content,
  executionProcessId,
  taskAttempt,
  conversationId,
}: {
  content: string;
  executionProcessId?: string;
  taskAttempt?: WorkspaceWithSession;
  conversationId?: string;
}) => {
  const [isEditing, setIsEditing] = useState(false);
  const { capabilities } = useUserSystem();
  const { activeRetryProcessId, setActiveRetryProcessId, isProcessGreyed } =
    useRetryUi();
  const { isAttemptRunning } = useAttemptExecution(taskAttempt?.id);

  const canFork = !!(
    taskAttempt?.session?.executor &&
    capabilities?.[taskAttempt.session.executor]?.includes(
      BaseAgentCapability.SESSION_FORK
    )
  );

  const startRetry = () => {
    if (!executionProcessId || !taskAttempt) return;
    setIsEditing(true);
    setActiveRetryProcessId(executionProcessId);
  };

  const onCancelled = () => {
    setIsEditing(false);
    setActiveRetryProcessId(null);
  };

  const showRetryEditor =
    !!executionProcessId &&
    isEditing &&
    activeRetryProcessId === executionProcessId;
  const greyed =
    !!executionProcessId &&
    isProcessGreyed(executionProcessId) &&
    !showRetryEditor;

  // Only show retry button when allowed (has process, can fork, not running)
  const canRetry = executionProcessId && canFork && !isAttemptRunning;

  const { t } = useTranslation('common');

  return (
    <div className={`py-2 ${greyed ? 'opacity-50 pointer-events-none' : ''}`}>
      <div className="bg-muted/50 border-l-4 border-primary/50 px-4 py-2 text-sm rounded-r-md">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
          <User className="h-3 w-3" />
          <span>{t('conversation.role.user')}</span>
        </div>
        <div className="py-3">
          {showRetryEditor && taskAttempt ? (
            <RetryEditorInline
              attempt={taskAttempt}
              executionProcessId={executionProcessId}
              initialContent={content}
              onCancelled={onCancelled}
            />
          ) : (
            <WYSIWYGEditor
              value={content}
              disabled
              className="whitespace-pre-wrap break-words flex flex-col gap-1 font-light"
              taskAttemptId={taskAttempt?.id}
              conversationId={conversationId}
              onEdit={canRetry ? startRetry : undefined}
            />
          )}
        </div>
      </div>
    </div>
  );
};

export default UserMessage;
