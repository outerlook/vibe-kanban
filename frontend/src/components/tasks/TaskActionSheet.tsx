import { motion, AnimatePresence } from 'framer-motion';
import { X } from 'lucide-react';
import { useHotkeysContext } from 'react-hotkeys-hook';
import { useEffect, useCallback } from 'react';
import { TASK_STATUSES } from '@/constants/taskStatuses';
import { statusLabels, statusBoardColors } from '@/utils/statusLabels';
import { useKeyExit, Scope } from '@/keyboard';
import type { TaskWithAttemptStatus, TaskStatus } from 'shared/types';

export interface TaskActionSheetProps {
  task: TaskWithAttemptStatus | null;
  isOpen: boolean;
  onClose: () => void;
  onStatusChange: (taskId: string, newStatus: TaskStatus) => void;
}

export function TaskActionSheet({
  task,
  isOpen,
  onClose,
  onStatusChange,
}: TaskActionSheetProps) {
  const { enableScope, disableScope } = useHotkeysContext();

  // Manage dialog scope when open/closed
  useEffect(() => {
    if (isOpen) {
      enableScope(Scope.DIALOG);
      disableScope(Scope.KANBAN);
      disableScope(Scope.PROJECTS);
    } else {
      disableScope(Scope.DIALOG);
      enableScope(Scope.KANBAN);
      enableScope(Scope.PROJECTS);
    }
    return () => {
      disableScope(Scope.DIALOG);
      enableScope(Scope.KANBAN);
      enableScope(Scope.PROJECTS);
    };
  }, [isOpen, enableScope, disableScope]);

  // Close on Escape key
  useKeyExit(
    () => {
      onClose();
    },
    {
      scope: Scope.DIALOG,
      when: () => isOpen,
    }
  );

  const handleStatusSelect = useCallback(
    (status: TaskStatus) => {
      if (task && status !== task.status) {
        onStatusChange(task.id, status);
        onClose();
      }
    },
    [task, onStatusChange, onClose]
  );

  const handleBackdropClick = useCallback(() => {
    onClose();
  }, [onClose]);

  // Filter out the current status from options
  const availableStatuses = task
    ? TASK_STATUSES.filter((status) => status !== task.status)
    : [];

  return (
    <AnimatePresence>
      {isOpen && task && (
        <div className="fixed inset-0 z-[9999] flex items-end justify-center">
          {/* Backdrop */}
          <motion.div
            className="absolute inset-0 bg-black/50"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            onClick={handleBackdropClick}
          />

          {/* Sheet */}
          <motion.div
            className="relative z-[9999] w-full max-w-lg bg-primary rounded-t-2xl shadow-xl"
            initial={{ y: '100%' }}
            animate={{ y: 0 }}
            exit={{ y: '100%' }}
            transition={{
              type: 'spring',
              damping: 30,
              stiffness: 300,
              duration: 0.25,
            }}
          >
            {/* Drag handle indicator */}
            <div className="flex justify-center pt-3 pb-1">
              <div className="w-10 h-1 rounded-full bg-muted-foreground/30" />
            </div>

            {/* Header */}
            <div className="flex items-start justify-between px-4 pb-3">
              <div className="flex-1 min-w-0 pr-4">
                <h3 className="text-base font-semibold text-foreground truncate">
                  {task.title}
                </h3>
                <p className="text-sm text-muted-foreground mt-1 flex items-center gap-2">
                  <span
                    className="inline-block w-2.5 h-2.5 rounded-full"
                    style={{
                      backgroundColor: `var(${statusBoardColors[task.status]})`,
                    }}
                  />
                  {statusLabels[task.status]}
                </p>
              </div>
              <button
                type="button"
                className="shrink-0 p-2 -mr-2 -mt-1 rounded-full hover:bg-muted transition-colors"
                onClick={onClose}
                aria-label="Close"
              >
                <X className="h-5 w-5 text-muted-foreground" />
              </button>
            </div>

            {/* Divider */}
            <div className="h-px bg-border" />

            {/* Status options */}
            <div className="py-2 pb-safe">
              <p className="px-4 py-2 text-xs font-medium text-muted-foreground uppercase tracking-wider">
                Move to
              </p>
              {availableStatuses.map((status) => (
                <button
                  key={status}
                  type="button"
                  className="w-full flex items-center gap-3 px-4 py-3 hover:bg-muted active:bg-muted/80 transition-colors"
                  onClick={() => handleStatusSelect(status)}
                >
                  <span
                    className="inline-block w-3 h-3 rounded-full shrink-0"
                    style={{
                      backgroundColor: `var(${statusBoardColors[status]})`,
                    }}
                  />
                  <span className="text-sm font-medium text-foreground">
                    {statusLabels[status]}
                  </span>
                </button>
              ))}
            </div>

            {/* Bottom safe area padding for mobile */}
            <div className="h-4" />
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}
