import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { ReactNode } from 'react';
import type { QuestionAnswer, ToolStatus } from 'shared/types';
import { Button } from '@/components/ui/button';
import { approvalsApi } from '@/lib/api';
import { Check, X } from 'lucide-react';

import { useHotkeysContext } from 'react-hotkeys-hook';
import { TabNavContext } from '@/contexts/TabNavigationContext';
import { useKeyApproveRequest, useKeyDenyApproval, Scope } from '@/keyboard';
import { QuestionRenderer } from './QuestionRenderer';

interface PendingUserQuestionEntryProps {
  pendingStatus: Extract<ToolStatus, { status: 'pending_user_input' }>;
  executionProcessId?: string;
  children: ReactNode;
}

function useApprovalCountdown(
  requestedAt: string | number | Date,
  timeoutAt: string | number | Date,
  paused: boolean
) {
  const totalSeconds = useMemo(() => {
    const total = Math.floor(
      (new Date(timeoutAt).getTime() - new Date(requestedAt).getTime()) / 1000
    );
    return Math.max(1, total);
  }, [requestedAt, timeoutAt]);

  const [timeLeft, setTimeLeft] = useState<number>(() => {
    const remaining = new Date(timeoutAt).getTime() - Date.now();
    return Math.max(0, Math.floor(remaining / 1000));
  });

  useEffect(() => {
    if (paused) return;
    const id = window.setInterval(() => {
      const remaining = new Date(timeoutAt).getTime() - Date.now();
      const next = Math.max(0, Math.floor(remaining / 1000));
      setTimeLeft(next);
      if (next <= 0) window.clearInterval(id);
    }, 1000);

    return () => window.clearInterval(id);
  }, [timeoutAt, paused]);

  const percent = useMemo(
    () =>
      Math.max(0, Math.min(100, Math.round((timeLeft / totalSeconds) * 100))),
    [timeLeft, totalSeconds]
  );

  return { timeLeft, percent };
}

const PendingUserQuestionEntry = ({
  pendingStatus,
  executionProcessId,
  children,
}: PendingUserQuestionEntryProps) => {
  const [isResponding, setIsResponding] = useState(false);
  const [hasResponded, setHasResponded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [answers, setAnswers] = useState<Map<number, QuestionAnswer>>(
    new Map()
  );

  const { enableScope, disableScope, activeScopes } = useHotkeysContext();
  const tabNav = useContext(TabNavContext);
  const isLogsTabActive = tabNav ? tabNav.activeTab === 'logs' : true;
  const dialogScopeActive = activeScopes.includes(Scope.DIALOG);
  const shouldControlScopes = isLogsTabActive && !dialogScopeActive;
  const approvalsScopeEnabledRef = useRef(false);
  const dialogScopeActiveRef = useRef(dialogScopeActive);

  useEffect(() => {
    dialogScopeActiveRef.current = dialogScopeActive;
  }, [dialogScopeActive]);

  const { timeLeft } = useApprovalCountdown(
    pendingStatus.requested_at,
    pendingStatus.timeout_at,
    hasResponded
  );

  const disabled = isResponding || hasResponded || timeLeft <= 0;

  // Validation: at least one selection per question
  const isValid = useMemo(() => {
    return pendingStatus.questions.every((_, index) => {
      const answer = answers.get(index);
      if (!answer) return false;
      // Has at least one selected option or has other_text
      return answer.selected_indices.length > 0 || (answer.other_text && answer.other_text.length > 0);
    });
  }, [pendingStatus.questions, answers]);

  const shouldEnableApprovalsScope = shouldControlScopes && !disabled;

  useEffect(() => {
    const shouldEnable = shouldEnableApprovalsScope;

    if (shouldEnable && !approvalsScopeEnabledRef.current) {
      enableScope(Scope.APPROVALS);
      disableScope(Scope.KANBAN);
      approvalsScopeEnabledRef.current = true;
    } else if (!shouldEnable && approvalsScopeEnabledRef.current) {
      disableScope(Scope.APPROVALS);
      if (!dialogScopeActive) {
        enableScope(Scope.KANBAN);
      }
      approvalsScopeEnabledRef.current = false;
    }

    return () => {
      if (approvalsScopeEnabledRef.current) {
        disableScope(Scope.APPROVALS);
        if (!dialogScopeActiveRef.current) {
          enableScope(Scope.KANBAN);
        }
        approvalsScopeEnabledRef.current = false;
      }
    };
  }, [
    disableScope,
    enableScope,
    dialogScopeActive,
    shouldEnableApprovalsScope,
  ]);

  const handleAnswerChange = useCallback(
    (questionIndex: number, answer: QuestionAnswer) => {
      setAnswers((prev) => {
        const next = new Map(prev);
        next.set(questionIndex, answer);
        return next;
      });
    },
    []
  );

  const respond = useCallback(
    async (approved: boolean) => {
      if (disabled) return;
      if (!executionProcessId) {
        setError('Missing executionProcessId');
        return;
      }

      setIsResponding(true);
      setError(null);

      try {
        const answersArray = approved
          ? Array.from(answers.values())
          : undefined;

        await approvalsApi.respond(pendingStatus.approval_id, {
          execution_process_id: executionProcessId,
          status: approved
            ? { status: 'approved' }
            : { status: 'denied', reason: 'User cancelled' },
          answers: answersArray,
        });
        setHasResponded(true);
      } catch (e: unknown) {
        console.error('User question respond failed:', e);
        const errorMessage =
          e instanceof Error ? e.message : 'Failed to send response';
        setError(errorMessage);
      } finally {
        setIsResponding(false);
      }
    },
    [disabled, executionProcessId, pendingStatus.approval_id, answers]
  );

  const handleSubmit = useCallback(() => {
    if (!isValid || disabled) return;
    respond(true);
  }, [isValid, disabled, respond]);

  const handleCancel = useCallback(() => {
    if (disabled) return;
    respond(false);
  }, [disabled, respond]);

  useKeyApproveRequest(handleSubmit, {
    scope: Scope.APPROVALS,
    when: () => shouldEnableApprovalsScope && isValid,
    preventDefault: true,
  });

  useKeyDenyApproval(handleCancel, {
    scope: Scope.APPROVALS,
    when: () => shouldEnableApprovalsScope && !hasResponded,
    enableOnFormTags: ['input', 'INPUT'],
    preventDefault: true,
  });

  return (
    <div className="relative mt-3">
      <div className="overflow-hidden">
        {children}

        <div className="bg-background px-4 py-3 text-xs sm:text-sm">
          <div className="flex flex-col gap-4">
            {pendingStatus.questions.map((question, index) => (
              <QuestionRenderer
                key={index}
                question={question}
                questionIndex={index}
                answer={answers.get(index)}
                onAnswerChange={(answer) => handleAnswerChange(index, answer)}
              />
            ))}

            {error && (
              <div
                className="text-xs text-red-600"
                role="alert"
                aria-live="polite"
              >
                {error}
              </div>
            )}

            <div className="flex items-center justify-between gap-2 pt-2 border-t">
              <div className="text-muted-foreground text-xs">
                {timeLeft > 0
                  ? `${timeLeft}s remaining`
                  : 'Timed out'}
              </div>
              <div className="flex items-center gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleCancel}
                  disabled={disabled}
                  aria-label={isResponding ? 'Cancelling...' : 'Cancel'}
                >
                  <X className="h-4 w-4 mr-1" />
                  Cancel
                </Button>
                <Button
                  size="sm"
                  onClick={handleSubmit}
                  disabled={disabled || !isValid}
                  aria-label={isResponding ? 'Submitting...' : 'Submit'}
                  aria-busy={isResponding}
                >
                  <Check className="h-4 w-4 mr-1" />
                  Submit
                </Button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default PendingUserQuestionEntry;
