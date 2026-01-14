import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Code2, GitBranch, MoreHorizontal } from 'lucide-react';
import type { TaskWithAttemptStatus } from 'shared/types';
import type { Workspace } from 'shared/types';
import { EditorType } from 'shared/types';
import { useOpenInEditor } from '@/hooks/useOpenInEditor';
import { DeleteTaskConfirmationDialog } from '@/components/dialogs/tasks/DeleteTaskConfirmationDialog';
import { ViewProcessesDialog } from '@/components/dialogs/tasks/ViewProcessesDialog';
import { ViewRelatedTasksDialog } from '@/components/dialogs/tasks/ViewRelatedTasksDialog';
import { CreateAttemptDialog } from '@/components/dialogs/tasks/CreateAttemptDialog';
import { GitActionsDialog } from '@/components/dialogs/tasks/GitActionsDialog';
import { EditBranchNameDialog } from '@/components/dialogs/tasks/EditBranchNameDialog';
import { ShareDialog } from '@/components/dialogs/tasks/ShareDialog';
import { ReassignDialog } from '@/components/dialogs/tasks/ReassignDialog';
import { StopShareTaskDialog } from '@/components/dialogs/tasks/StopShareTaskDialog';
import { DependencyTreeDialog } from '@/components/dialogs/tasks/DependencyTreeDialog';
import { AddDependencyDialog } from '@/components/dialogs/tasks/AddDependencyDialog';
import { useProject } from '@/contexts/ProjectContext';
import { openTaskForm } from '@/lib/openTaskForm';
import { IdeIcon, getIdeName } from '@/components/ide/IdeIcon';

import { useNavigate } from 'react-router-dom';
import type { SharedTaskRecord } from '@/hooks/useProjectTasks';
import { useAuth, useCustomEditors, useTaskDependencies } from '@/hooks';

const PREFERRED_EDITOR_KEY = 'preferredEditor';
const CUSTOM_EDITOR_PREFIX = 'custom:';

interface ActionsDropdownProps {
  task?: TaskWithAttemptStatus | null;
  attempt?: Workspace | null;
  sharedTask?: SharedTaskRecord;
}

export function ActionsDropdown({
  task,
  attempt,
  sharedTask,
}: ActionsDropdownProps) {
  const { t } = useTranslation('tasks');
  const { projectId } = useProject();
  const openInEditor = useOpenInEditor(attempt?.id);
  const navigate = useNavigate();
  const { userId, isSignedIn } = useAuth();
  const [isAddDependencyOpen, setIsAddDependencyOpen] = useState(false);
  const dependencyQuery = useTaskDependencies(task?.id, {
    enabled: isAddDependencyOpen && Boolean(task?.id),
  });
  const existingDependencyIds = useMemo(
    () => dependencyQuery.data?.blocked_by?.map((dep) => dep.id) ?? [],
    [dependencyQuery.data]
  );
  const isDependenciesLoading = dependencyQuery.isLoading;
  const { data: customEditors = [] } = useCustomEditors();

  const editorOptions = useMemo(() => {
    const builtIn = Object.values(EditorType)
      .filter((type) => type !== EditorType.CUSTOM)
      .map((editorType) => ({
        value: editorType,
        label: getIdeName(editorType),
        icon: <IdeIcon editorType={editorType} className="h-3.5 w-3.5" />,
        isCustom: false,
      }));

    const custom = customEditors.map((editor) => ({
      value: `${CUSTOM_EDITOR_PREFIX}${editor.id}`,
      label: editor.name,
      icon: <Code2 className="h-3.5 w-3.5" />,
      isCustom: true,
    }));

    return [...builtIn, ...custom];
  }, [customEditors]);

  const hasAttemptActions = Boolean(attempt);
  const hasTaskActions = Boolean(task);
  const isShared = Boolean(sharedTask);
  const canEditShared = (!isShared && !task?.shared_task_id) || isSignedIn;

  const handleEdit = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    openTaskForm({ mode: 'edit', projectId, task });
  };

  const handleDuplicate = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    openTaskForm({ mode: 'duplicate', projectId, initialTask: task });
  };

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task) return;
    try {
      await DeleteTaskConfirmationDialog.show({
        task,
        projectId,
      });
    } catch {
      // User cancelled or error occurred
    }
  };

  const handleOpenInEditor = (editorValue: string) => {
    if (!attempt?.id) return;
    localStorage.setItem(PREFERRED_EDITOR_KEY, editorValue);
    openInEditor({ editorType: editorValue as EditorType });
  };

  const handleViewProcesses = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id) return;
    ViewProcessesDialog.show({ attemptId: attempt.id });
  };

  const handleViewRelatedTasks = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id || !projectId) return;
    ViewRelatedTasksDialog.show({
      attemptId: attempt.id,
      projectId,
      attempt,
      onNavigateToTask: (taskId: string) => {
        if (projectId) {
          navigate(`/projects/${projectId}/tasks/${taskId}/attempts/latest`);
        }
      },
    });
  };

  const handleCreateNewAttempt = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!task?.id) return;
    CreateAttemptDialog.show({
      taskId: task.id,
    });
  };

  const handleCreateSubtask = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !attempt) return;
    const baseBranch = attempt.branch;
    if (!baseBranch) return;
    openTaskForm({
      mode: 'subtask',
      projectId,
      parentTaskAttemptId: attempt.id,
      initialBaseBranch: baseBranch,
    });
  };

  const handleGitActions = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id || !task) return;
    GitActionsDialog.show({
      attemptId: attempt.id,
      task,
    });
  };

  const handleEditBranchName = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!attempt?.id) return;
    EditBranchNameDialog.show({
      attemptId: attempt.id,
      currentBranchName: attempt.branch,
    });
  };
  const handleShare = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!task || isShared) return;
    ShareDialog.show({ task });
  };

  const handleViewDependencyTree = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!task?.id) return;
    DependencyTreeDialog.show({ taskId: task.id });
  };

  const handleReassign = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!sharedTask) return;
    ReassignDialog.show({ sharedTask });
  };

  const handleStopShare = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!sharedTask) return;
    StopShareTaskDialog.show({ sharedTask });
  };

  const handleAddDependency = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!projectId || !task?.id) return;
    setIsAddDependencyOpen(true);
  };

  const canReassign =
    Boolean(task) &&
    Boolean(sharedTask) &&
    sharedTask?.assignee_user_id === userId;
  const canStopShare =
    Boolean(sharedTask) && sharedTask?.assignee_user_id === userId;

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="icon"
            aria-label="Actions"
            onPointerDown={(e) => e.stopPropagation()}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {hasAttemptActions && (
            <>
              <DropdownMenuLabel>{t('actionsMenu.attempt')}</DropdownMenuLabel>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger disabled={!attempt?.id}>
                  {t('actionsMenu.openInIde')}
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {editorOptions.map((option) => (
                    <DropdownMenuItem
                      key={option.value}
                      onClick={() => handleOpenInEditor(option.value)}
                    >
                      {option.icon}
                      <span className="ml-2">{option.label}</span>
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuItem
                disabled={!attempt?.id}
                onClick={handleViewProcesses}
              >
                {t('actionsMenu.viewProcesses')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!attempt?.id}
                onClick={handleViewRelatedTasks}
              >
                {t('actionsMenu.viewRelatedTasks')}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleCreateNewAttempt}>
                {t('actionsMenu.createNewAttempt')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!projectId || !attempt}
                onClick={handleCreateSubtask}
              >
                {t('actionsMenu.createSubtask')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!attempt?.id || !task}
                onClick={handleGitActions}
              >
                {t('actionsMenu.gitActions')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!attempt?.id}
                onClick={handleEditBranchName}
              >
                {t('actionsMenu.editBranchName')}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
            </>
          )}

          {hasTaskActions && (
            <>
              <DropdownMenuLabel>{t('actionsMenu.task')}</DropdownMenuLabel>
              <DropdownMenuItem
                disabled={!task}
                onClick={handleViewDependencyTree}
              >
                <GitBranch className="h-4 w-4 mr-2" />
                {t('actionsMenu.viewDependencyTree')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!projectId || !task}
                onClick={handleAddDependency}
              >
                {t('actionsMenu.addDependency')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!task || isShared}
                onClick={handleShare}
              >
                {t('actionsMenu.share')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!canReassign}
                onClick={handleReassign}
              >
                {t('actionsMenu.reassign')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!canStopShare}
                onClick={handleStopShare}
                className="text-destructive"
              >
                {t('actionsMenu.stopShare')}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                disabled={!projectId || !canEditShared}
                onClick={handleEdit}
              >
                {t('common:buttons.edit')}
              </DropdownMenuItem>
              <DropdownMenuItem disabled={!projectId} onClick={handleDuplicate}>
                {t('actionsMenu.duplicate')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!projectId || !canEditShared}
                onClick={handleDelete}
                className="text-destructive"
              >
                {t('common:buttons.delete')}
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {projectId && task?.id && (
        <AddDependencyDialog
          taskId={task.id}
          projectId={projectId}
          existingDependencyIds={existingDependencyIds}
          dependenciesLoading={isDependenciesLoading}
          open={isAddDependencyOpen}
          onOpenChange={setIsAddDependencyOpen}
        />
      )}
    </>
  );
}
