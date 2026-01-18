import { useCallback, useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';
import { isEqual } from 'lodash';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Checkbox } from '@/components/ui/checkbox';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Text } from '@/components/ui/text';
import { ListItem } from '@/components/ui/list-item';
import { SettingsSection } from '@/components/settings/SettingsSection';
import { SettingsField } from '@/components/settings/SettingsField';
import { SkeletonForm, SkeletonList } from '@/components/ui/loading-states';
import { FolderGit2, Loader2, Plus, Trash2, Users } from 'lucide-react';
import { useProjects } from '@/hooks/useProjects';
import { useProjectMutations } from '@/hooks/useProjectMutations';
import { useScriptPlaceholders } from '@/hooks/useScriptPlaceholders';
import { useDeleteTaskGroup, useTaskGroups } from '@/hooks/useTaskGroups';
import { CopyFilesField } from '@/components/projects/CopyFilesField';
import { AutoExpandingTextarea } from '@/components/ui/auto-expanding-textarea';
import { TaskGroupFormDialog } from '@/components/dialogs';
import { RepoPickerDialog } from '@/components/dialogs/shared/RepoPickerDialog';
import { projectsApi } from '@/lib/api';
import { repoBranchKeys } from '@/hooks/useRepoBranches';
import type { Project, ProjectRepo, Repo, UpdateProject } from 'shared/types';

interface ProjectFormState {
  name: string;
  dev_script: string;
  dev_script_working_dir: string;
  default_agent_working_dir: string;
}

interface RepoScriptsFormState {
  setup_script: string;
  parallel_setup_script: boolean;
  cleanup_script: string;
  copy_files: string;
}

function projectToFormState(project: Project): ProjectFormState {
  return {
    name: project.name,
    dev_script: project.dev_script ?? '',
    dev_script_working_dir: project.dev_script_working_dir ?? '',
    default_agent_working_dir: project.default_agent_working_dir ?? '',
  };
}

function projectRepoToScriptsFormState(
  projectRepo: ProjectRepo | null
): RepoScriptsFormState {
  return {
    setup_script: projectRepo?.setup_script ?? '',
    parallel_setup_script: projectRepo?.parallel_setup_script ?? false,
    cleanup_script: projectRepo?.cleanup_script ?? '',
    copy_files: projectRepo?.copy_files ?? '',
  };
}

export function ProjectSettings() {
  const [searchParams, setSearchParams] = useSearchParams();
  const projectIdParam = searchParams.get('projectId') ?? '';
  const { t } = useTranslation('settings');
  const queryClient = useQueryClient();

  // Fetch all projects
  const {
    projects,
    isLoading: projectsLoading,
    error: projectsError,
  } = useProjects();

  // Selected project state
  const [selectedProjectId, setSelectedProjectId] = useState<string>(
    searchParams.get('projectId') || ''
  );
  const [selectedProject, setSelectedProject] = useState<Project | null>(null);

  // Form state
  const [draft, setDraft] = useState<ProjectFormState | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  // Repositories state
  const [repositories, setRepositories] = useState<Repo[]>([]);
  const [loadingRepos, setLoadingRepos] = useState(false);
  const [repoError, setRepoError] = useState<string | null>(null);
  const [addingRepo, setAddingRepo] = useState(false);
  const [deletingRepoId, setDeletingRepoId] = useState<string | null>(null);

  // Task groups state
  const {
    data: taskGroups = [],
    isLoading: loadingTaskGroups,
    error: taskGroupsError,
  } = useTaskGroups(selectedProjectId);
  const deleteTaskGroup = useDeleteTaskGroup();
  const [deletingGroupId, setDeletingGroupId] = useState<string | null>(null);
  const [taskGroupError, setTaskGroupError] = useState<string | null>(null);

  // Scripts repo state (per-repo scripts)
  const [selectedScriptsRepoId, setSelectedScriptsRepoId] = useState<
    string | null
  >(null);
  const [selectedProjectRepo, setSelectedProjectRepo] =
    useState<ProjectRepo | null>(null);
  const [scriptsDraft, setScriptsDraft] = useState<RepoScriptsFormState | null>(
    null
  );
  const [loadingProjectRepo, setLoadingProjectRepo] = useState(false);
  const [savingScripts, setSavingScripts] = useState(false);
  const [scriptsSuccess, setScriptsSuccess] = useState(false);
  const [scriptsError, setScriptsError] = useState<string | null>(null);

  // Get OS-appropriate script placeholders
  const placeholders = useScriptPlaceholders();

  // Check for unsaved changes (project name)
  const hasUnsavedProjectChanges = useMemo(() => {
    if (!draft || !selectedProject) return false;
    return !isEqual(draft, projectToFormState(selectedProject));
  }, [draft, selectedProject]);

  // Check for unsaved script changes
  const hasUnsavedScriptsChanges = useMemo(() => {
    if (!scriptsDraft || !selectedProjectRepo) return false;
    return !isEqual(
      scriptsDraft,
      projectRepoToScriptsFormState(selectedProjectRepo)
    );
  }, [scriptsDraft, selectedProjectRepo]);

  // Combined check for any unsaved changes
  const hasUnsavedChanges =
    hasUnsavedProjectChanges || hasUnsavedScriptsChanges;

  // Handle project selection from dropdown
  const handleProjectSelect = useCallback(
    (id: string) => {
      // No-op if same project
      if (id === selectedProjectId) return;

      // Confirm if there are unsaved changes
      if (hasUnsavedChanges) {
        const confirmed = window.confirm(
          t('settings.projects.save.confirmSwitch')
        );
        if (!confirmed) return;

        // Clear local state before switching
        setDraft(null);
        setSelectedProject(null);
        setSuccess(false);
        setError(null);
      }

      // Update state and URL
      setSelectedProjectId(id);
      if (id) {
        setSearchParams({ projectId: id });
      } else {
        setSearchParams({});
      }
    },
    [hasUnsavedChanges, selectedProjectId, setSearchParams, t]
  );

  // Sync selectedProjectId when URL changes (with unsaved changes prompt)
  useEffect(() => {
    if (projectIdParam === selectedProjectId) return;

    // Confirm if there are unsaved changes
    if (hasUnsavedChanges) {
      const confirmed = window.confirm(
        t('settings.projects.save.confirmSwitch')
      );
      if (!confirmed) {
        // Revert URL to previous value
        if (selectedProjectId) {
          setSearchParams({ projectId: selectedProjectId });
        } else {
          setSearchParams({});
        }
        return;
      }

      // Clear local state before switching
      setDraft(null);
      setSelectedProject(null);
      setSuccess(false);
      setError(null);
    }

    setSelectedProjectId(projectIdParam);
  }, [
    projectIdParam,
    hasUnsavedChanges,
    selectedProjectId,
    setSearchParams,
    t,
  ]);

  // Populate draft from server data
  useEffect(() => {
    if (!projects) return;

    const nextProject = selectedProjectId
      ? projects.find((p) => p.id === selectedProjectId)
      : null;

    setSelectedProject((prev) =>
      prev?.id === nextProject?.id ? prev : (nextProject ?? null)
    );

    if (!nextProject) {
      if (!hasUnsavedChanges) setDraft(null);
      return;
    }

    if (hasUnsavedChanges) return;

    setDraft(projectToFormState(nextProject));
  }, [projects, selectedProjectId, hasUnsavedChanges]);

  // Warn on tab close/navigation with unsaved changes
  useEffect(() => {
    const handler = (e: BeforeUnloadEvent) => {
      if (hasUnsavedChanges) {
        e.preventDefault();
        e.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [hasUnsavedChanges]);

  // Fetch repositories when project changes
  useEffect(() => {
    if (!selectedProjectId) {
      setRepositories([]);
      return;
    }

    setLoadingRepos(true);
    setRepoError(null);
    projectsApi
      .getRepositories(selectedProjectId)
      .then(setRepositories)
      .catch((err) => {
        setRepoError(
          err instanceof Error ? err.message : 'Failed to load repositories'
        );
        setRepositories([]);
      })
      .finally(() => setLoadingRepos(false));
  }, [selectedProjectId]);

  // Auto-select first repository for scripts when repositories load
  useEffect(() => {
    if (repositories.length > 0 && !selectedScriptsRepoId) {
      setSelectedScriptsRepoId(repositories[0].id);
    }
    // Clear selection if repo was deleted
    if (
      selectedScriptsRepoId &&
      !repositories.some((r) => r.id === selectedScriptsRepoId)
    ) {
      setSelectedScriptsRepoId(repositories[0]?.id ?? null);
    }
  }, [repositories, selectedScriptsRepoId]);

  // Reset scripts selection when project changes
  useEffect(() => {
    setSelectedScriptsRepoId(null);
    setSelectedProjectRepo(null);
    setScriptsDraft(null);
    setScriptsError(null);
    setTaskGroupError(null);
  }, [selectedProjectId]);

  // Fetch ProjectRepo scripts when selected scripts repo changes
  useEffect(() => {
    if (!selectedProjectId || !selectedScriptsRepoId) {
      setSelectedProjectRepo(null);
      setScriptsDraft(null);
      return;
    }

    setLoadingProjectRepo(true);
    setScriptsError(null);
    projectsApi
      .getRepository(selectedProjectId, selectedScriptsRepoId)
      .then((projectRepo) => {
        setSelectedProjectRepo(projectRepo);
        setScriptsDraft(projectRepoToScriptsFormState(projectRepo));
      })
      .catch((err) => {
        setScriptsError(
          err instanceof Error
            ? err.message
            : 'Failed to load repository scripts'
        );
        setSelectedProjectRepo(null);
        setScriptsDraft(null);
      })
      .finally(() => setLoadingProjectRepo(false));
  }, [selectedProjectId, selectedScriptsRepoId]);

  const handleAddRepository = async () => {
    if (!selectedProjectId) return;

    const repo = await RepoPickerDialog.show({
      title: 'Select Git Repository',
      description: 'Choose a git repository to add to this project',
    });

    if (!repo) return;

    if (repositories.some((r) => r.id === repo.id)) {
      return;
    }

    setAddingRepo(true);
    setRepoError(null);
    try {
      const newRepo = await projectsApi.addRepository(selectedProjectId, {
        display_name: repo.display_name,
        git_repo_path: repo.path,
      });
      setRepositories((prev) => [...prev, newRepo]);
      queryClient.invalidateQueries({
        queryKey: ['projectRepositories', selectedProjectId],
      });
      queryClient.invalidateQueries({
        queryKey: repoBranchKeys.byRepo(newRepo.id),
      });
    } catch (err) {
      setRepoError(
        err instanceof Error ? err.message : 'Failed to add repository'
      );
    } finally {
      setAddingRepo(false);
    }
  };

  const handleDeleteRepository = async (repoId: string) => {
    if (!selectedProjectId) return;

    setDeletingRepoId(repoId);
    setRepoError(null);
    try {
      await projectsApi.deleteRepository(selectedProjectId, repoId);
      setRepositories((prev) => prev.filter((r) => r.id !== repoId));
      queryClient.invalidateQueries({
        queryKey: ['projectRepositories', selectedProjectId],
      });
      queryClient.invalidateQueries({
        queryKey: repoBranchKeys.byRepo(repoId),
      });
    } catch (err) {
      setRepoError(
        err instanceof Error ? err.message : 'Failed to delete repository'
      );
    } finally {
      setDeletingRepoId(null);
    }
  };

  const handleDeleteTaskGroup = async (groupId: string, groupName: string) => {
    if (!selectedProjectId) return;

    const confirmed = window.confirm(
      `Delete task group "${groupName}"? This cannot be undone.`
    );
    if (!confirmed) return;

    setDeletingGroupId(groupId);
    setTaskGroupError(null);
    try {
      await deleteTaskGroup.mutateAsync({
        groupId,
        projectId: selectedProjectId,
      });
    } catch (err) {
      setTaskGroupError(
        err instanceof Error ? err.message : 'Failed to delete task group'
      );
    } finally {
      setDeletingGroupId(null);
    }
  };

  const { updateProject } = useProjectMutations({
    onUpdateSuccess: (updatedProject: Project) => {
      // Update local state with fresh data from server
      setSelectedProject(updatedProject);
      setDraft(projectToFormState(updatedProject));
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
      setSaving(false);
    },
    onUpdateError: (err) => {
      setError(
        err instanceof Error ? err.message : 'Failed to save project settings'
      );
      setSaving(false);
    },
  });

  const handleSave = async () => {
    if (!draft || !selectedProject) return;

    setSaving(true);
    setError(null);
    setSuccess(false);

    try {
      const updateData: UpdateProject = {
        name: draft.name.trim(),
        dev_script: draft.dev_script.trim() || null,
        dev_script_working_dir: draft.dev_script_working_dir.trim() || null,
        default_agent_working_dir:
          draft.default_agent_working_dir.trim() || null,
      };

      updateProject.mutate({
        projectId: selectedProject.id,
        data: updateData,
      });
    } catch (err) {
      setError(t('settings.projects.save.error'));
      console.error('Error saving project settings:', err);
      setSaving(false);
    }
  };

  const handleSaveScripts = async () => {
    if (!scriptsDraft || !selectedProjectId || !selectedScriptsRepoId) return;

    setSavingScripts(true);
    setScriptsError(null);
    setScriptsSuccess(false);

    try {
      const updatedRepo = await projectsApi.updateRepository(
        selectedProjectId,
        selectedScriptsRepoId,
        {
          setup_script: scriptsDraft.setup_script.trim() || null,
          cleanup_script: scriptsDraft.cleanup_script.trim() || null,
          copy_files: scriptsDraft.copy_files.trim() || null,
          parallel_setup_script: scriptsDraft.parallel_setup_script,
        }
      );
      setSelectedProjectRepo(updatedRepo);
      setScriptsDraft(projectRepoToScriptsFormState(updatedRepo));
      setScriptsSuccess(true);
      setTimeout(() => setScriptsSuccess(false), 3000);
    } catch (err) {
      setScriptsError(
        err instanceof Error ? err.message : 'Failed to save scripts'
      );
    } finally {
      setSavingScripts(false);
    }
  };

  const handleDiscard = () => {
    if (!selectedProject) return;
    setDraft(projectToFormState(selectedProject));
  };

  const handleDiscardScripts = () => {
    if (!selectedProjectRepo) return;
    setScriptsDraft(projectRepoToScriptsFormState(selectedProjectRepo));
  };

  const updateDraft = (updates: Partial<ProjectFormState>) => {
    setDraft((prev) => {
      if (!prev) return prev;
      return { ...prev, ...updates };
    });
  };

  const updateScriptsDraft = (updates: Partial<RepoScriptsFormState>) => {
    setScriptsDraft((prev) => {
      if (!prev) return prev;
      return { ...prev, ...updates };
    });
  };

  if (projectsLoading) {
    return (
      <div className="space-y-6">
        <Card>
          <CardHeader>
            <CardTitle>{t('settings.projects.title')}</CardTitle>
          </CardHeader>
          <CardContent>
            <SkeletonForm fields={1} />
          </CardContent>
        </Card>
        <SkeletonForm fields={4} />
      </div>
    );
  }

  if (projectsError) {
    return (
      <div className="py-8">
        <Alert variant="destructive">
          <AlertDescription>
            {projectsError instanceof Error
              ? projectsError.message
              : t('settings.projects.loadError')}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {success && (
        <Alert variant="success">
          <AlertDescription className="font-medium">
            {t('settings.projects.save.success')}
          </AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.projects.title')}</CardTitle>
          <Text variant="secondary" size="sm" as="p">
            {t('settings.projects.description')}
          </Text>
        </CardHeader>
        <CardContent className="space-y-4">
          <SettingsField
            label={t('settings.projects.selector.label')}
            description={t('settings.projects.selector.helper')}
            htmlFor="project-selector"
          >
            <Select
              value={selectedProjectId}
              onValueChange={handleProjectSelect}
            >
              <SelectTrigger id="project-selector">
                <SelectValue
                  placeholder={t('settings.projects.selector.placeholder')}
                />
              </SelectTrigger>
              <SelectContent>
                {projects && projects.length > 0 ? (
                  projects.map((project) => (
                    <SelectItem key={project.id} value={project.id}>
                      {project.name}
                    </SelectItem>
                  ))
                ) : (
                  <SelectItem value="no-projects" disabled>
                    {t('settings.projects.selector.noProjects')}
                  </SelectItem>
                )}
              </SelectContent>
            </Select>
          </SettingsField>
        </CardContent>
      </Card>

      {selectedProject && draft && (
        <>
          {/* General Settings */}
          <SettingsSection
            id="project-general"
            title={t('settings.projects.general.title')}
            description={t('settings.projects.general.description')}
            collapsible
            defaultExpanded
          >
            <div className="space-y-4">
              <SettingsField
                label={t('settings.projects.general.name.label')}
                description={t('settings.projects.general.name.helper')}
                htmlFor="project-name"
              >
                <Input
                  id="project-name"
                  type="text"
                  value={draft.name}
                  onChange={(e) => updateDraft({ name: e.target.value })}
                  placeholder={t('settings.projects.general.name.placeholder')}
                  required
                />
              </SettingsField>

              <SettingsField
                label={t('settings.projects.scripts.dev.label')}
                description={t('settings.projects.scripts.dev.helper')}
                htmlFor="dev-script"
              >
                <AutoExpandingTextarea
                  id="dev-script"
                  value={draft.dev_script}
                  onChange={(e) => updateDraft({ dev_script: e.target.value })}
                  placeholder={placeholders.dev}
                  maxRows={12}
                  className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring font-mono"
                />
              </SettingsField>

              <SettingsField
                label={t('settings.projects.scripts.devWorkingDir.label')}
                description={t('settings.projects.scripts.devWorkingDir.helper')}
                htmlFor="dev-script-working-dir"
              >
                <Input
                  id="dev-script-working-dir"
                  value={draft.dev_script_working_dir}
                  onChange={(e) =>
                    updateDraft({ dev_script_working_dir: e.target.value })
                  }
                  placeholder={t(
                    'settings.projects.scripts.devWorkingDir.placeholder'
                  )}
                  className="font-mono"
                />
              </SettingsField>

              <SettingsField
                label={t('settings.projects.scripts.agentWorkingDir.label')}
                description={t(
                  'settings.projects.scripts.agentWorkingDir.helper'
                )}
                htmlFor="agent-working-dir"
              >
                <Input
                  id="agent-working-dir"
                  value={draft.default_agent_working_dir}
                  onChange={(e) =>
                    updateDraft({ default_agent_working_dir: e.target.value })
                  }
                  placeholder={t(
                    'settings.projects.scripts.agentWorkingDir.placeholder'
                  )}
                  className="font-mono"
                />
              </SettingsField>

              {/* Save Button */}
              <div className="flex items-center justify-between pt-4 border-t">
                {hasUnsavedProjectChanges ? (
                  <Text variant="secondary" size="sm">
                    {t('settings.projects.save.unsavedChanges')}
                  </Text>
                ) : (
                  <span />
                )}
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    onClick={handleDiscard}
                    disabled={saving || !hasUnsavedProjectChanges}
                  >
                    {t('settings.projects.save.discard')}
                  </Button>
                  <Button
                    onClick={handleSave}
                    disabled={saving || !hasUnsavedProjectChanges}
                  >
                    {saving ? (
                      <>
                        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                        {t('settings.projects.save.saving')}
                      </>
                    ) : (
                      t('settings.projects.save.button')
                    )}
                  </Button>
                </div>
              </div>
              {error && (
                <Alert variant="destructive">
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              )}
              {success && (
                <Alert>
                  <AlertDescription>
                    {t('settings.projects.save.success')}
                  </AlertDescription>
                </Alert>
              )}
            </div>
          </SettingsSection>

          {/* Repositories Section */}
          <SettingsSection
            id="project-repos"
            title="Repositories"
            description="Manage the git repositories in this project"
            collapsible
            defaultExpanded={false}
            badge={
              repositories.length > 0
                ? { label: String(repositories.length) }
                : undefined
            }
          >
            <div className="space-y-4">
              {repoError && (
                <Alert variant="destructive">
                  <AlertDescription>{repoError}</AlertDescription>
                </Alert>
              )}

              {loadingRepos ? (
                <SkeletonList items={3} showIcon showSecondaryText />
              ) : (
                <div className="space-y-1 border rounded-md divide-y">
                  {repositories.map((repo) => (
                    <ListItem
                      key={repo.id}
                      icon={<FolderGit2 className="h-4 w-4" />}
                      title={repo.display_name}
                      subtitle={repo.path}
                      actions={
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteRepository(repo.id)}
                          disabled={deletingRepoId === repo.id}
                          title="Delete repository"
                        >
                          {deletingRepoId === repo.id ? (
                            <Loader2 className="h-4 w-4 animate-spin" />
                          ) : (
                            <Trash2 className="h-4 w-4" />
                          )}
                        </Button>
                      }
                    />
                  ))}

                  {repositories.length === 0 && !loadingRepos && (
                    <div className="text-center py-4">
                      <Text variant="secondary" size="sm">
                        No repositories configured
                      </Text>
                    </div>
                  )}
                </div>
              )}

              <Button
                variant="outline"
                size="sm"
                onClick={handleAddRepository}
                disabled={addingRepo}
                className="w-full"
              >
                {addingRepo ? (
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                ) : (
                  <Plus className="h-4 w-4 mr-2" />
                )}
                Add Repository
              </Button>
            </div>
          </SettingsSection>

          {/* Task Groups Section */}
          <SettingsSection
            id="project-groups"
            title="Task Groups"
            description="Manage groups used to organize tasks in this project"
            collapsible
            defaultExpanded={false}
            badge={
              taskGroups.length > 0
                ? { label: String(taskGroups.length) }
                : undefined
            }
          >
            <div className="space-y-4">
              {(taskGroupError || taskGroupsError) && (
                <Alert variant="destructive">
                  <AlertDescription>
                    {taskGroupError ??
                      (taskGroupsError instanceof Error
                        ? taskGroupsError.message
                        : 'Failed to load task groups')}
                  </AlertDescription>
                </Alert>
              )}

              {loadingTaskGroups ? (
                <SkeletonList items={3} showIcon showSecondaryText />
              ) : (
                <div className="space-y-1 border rounded-md divide-y">
                  {taskGroups.map((group) => (
                    <ListItem
                      key={group.id}
                      icon={<Users className="h-4 w-4" />}
                      title={group.name}
                      subtitle={`Base branch: ${group.base_branch || 'None'}`}
                      actions={
                        <div className="flex items-center gap-2">
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={() =>
                              TaskGroupFormDialog.show({
                                mode: 'edit',
                                projectId: selectedProject.id,
                                group,
                              })
                            }
                          >
                            Edit
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() =>
                              handleDeleteTaskGroup(group.id, group.name)
                            }
                            disabled={deletingGroupId === group.id}
                          >
                            {deletingGroupId === group.id ? (
                              <Loader2 className="h-4 w-4 animate-spin" />
                            ) : (
                              <>
                                <Trash2 className="h-4 w-4 mr-2" />
                                Delete
                              </>
                            )}
                          </Button>
                        </div>
                      }
                    />
                  ))}

                  {taskGroups.length === 0 && !loadingTaskGroups && (
                    <div className="text-center py-4">
                      <Text variant="secondary" size="sm">
                        No groups configured
                      </Text>
                    </div>
                  )}
                </div>
              )}

              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  TaskGroupFormDialog.show({
                    mode: 'create',
                    projectId: selectedProject.id,
                  })
                }
                className="w-full"
              >
                <Plus className="h-4 w-4 mr-2" />
                Create Group
              </Button>
            </div>
          </SettingsSection>

          {/* Repository Scripts Section */}
          <SettingsSection
            id="project-scripts"
            title={t('settings.projects.scripts.title')}
            description={t('settings.projects.scripts.description')}
            collapsible
            defaultExpanded={false}
          >
            <div className="space-y-4">
              {scriptsError && (
                <Alert variant="destructive">
                  <AlertDescription>{scriptsError}</AlertDescription>
                </Alert>
              )}

              {scriptsSuccess && (
                <Alert variant="success">
                  <AlertDescription className="font-medium">
                    Scripts saved successfully
                  </AlertDescription>
                </Alert>
              )}

              {repositories.length === 0 ? (
                <div className="text-center py-4">
                  <Text variant="secondary" size="sm">
                    Add a repository above to configure scripts
                  </Text>
                </div>
              ) : (
                <>
                  {/* Repository Selector for Scripts */}
                  <SettingsField
                    label="Repository"
                    description="Configure scripts for each repository separately"
                    htmlFor="scripts-repo-selector"
                  >
                    <Select
                      value={selectedScriptsRepoId ?? ''}
                      onValueChange={setSelectedScriptsRepoId}
                    >
                      <SelectTrigger id="scripts-repo-selector">
                        <SelectValue placeholder="Select a repository" />
                      </SelectTrigger>
                      <SelectContent>
                        {repositories.map((repo) => (
                          <SelectItem key={repo.id} value={repo.id}>
                            {repo.display_name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </SettingsField>

                  {loadingProjectRepo ? (
                    <SkeletonForm fields={3} />
                  ) : scriptsDraft ? (
                    <>
                      <SettingsField
                        label={t('settings.projects.scripts.setup.label')}
                        description={t('settings.projects.scripts.setup.helper')}
                        htmlFor="setup-script"
                      >
                        <AutoExpandingTextarea
                          id="setup-script"
                          value={scriptsDraft.setup_script}
                          onChange={(e) =>
                            updateScriptsDraft({ setup_script: e.target.value })
                          }
                          placeholder={placeholders.setup}
                          maxRows={12}
                          className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring font-mono"
                        />
                      </SettingsField>

                      <div className="flex items-center space-x-2">
                        <Checkbox
                          id="parallel-setup-script"
                          checked={scriptsDraft.parallel_setup_script}
                          onCheckedChange={(checked) =>
                            updateScriptsDraft({
                              parallel_setup_script: checked === true,
                            })
                          }
                          disabled={!scriptsDraft.setup_script.trim()}
                        />
                        <Label
                          htmlFor="parallel-setup-script"
                          className="text-sm font-normal cursor-pointer"
                        >
                          {t('settings.projects.scripts.setup.parallelLabel')}
                        </Label>
                      </div>
                      <Text variant="secondary" size="sm" as="p" className="pl-6">
                        {t('settings.projects.scripts.setup.parallelHelper')}
                      </Text>

                      <SettingsField
                        label={t('settings.projects.scripts.cleanup.label')}
                        description={t(
                          'settings.projects.scripts.cleanup.helper'
                        )}
                        htmlFor="cleanup-script"
                      >
                        <AutoExpandingTextarea
                          id="cleanup-script"
                          value={scriptsDraft.cleanup_script}
                          onChange={(e) =>
                            updateScriptsDraft({
                              cleanup_script: e.target.value,
                            })
                          }
                          placeholder={placeholders.cleanup}
                          maxRows={12}
                          className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring font-mono"
                        />
                      </SettingsField>

                      <SettingsField
                        label={t('settings.projects.scripts.copyFiles.label')}
                        description={t(
                          'settings.projects.scripts.copyFiles.helper'
                        )}
                      >
                        <CopyFilesField
                          value={scriptsDraft.copy_files}
                          onChange={(value) =>
                            updateScriptsDraft({ copy_files: value })
                          }
                          projectId={selectedProject.id}
                        />
                      </SettingsField>

                      {/* Scripts Save Buttons */}
                      <div className="flex items-center justify-between pt-4 border-t">
                        {hasUnsavedScriptsChanges ? (
                          <Text variant="secondary" size="sm">
                            {t('settings.projects.save.unsavedChanges')}
                          </Text>
                        ) : (
                          <span />
                        )}
                        <div className="flex gap-2">
                          <Button
                            variant="outline"
                            onClick={handleDiscardScripts}
                            disabled={
                              !hasUnsavedScriptsChanges || savingScripts
                            }
                          >
                            {t('settings.projects.save.discard')}
                          </Button>
                          <Button
                            onClick={handleSaveScripts}
                            disabled={
                              !hasUnsavedScriptsChanges || savingScripts
                            }
                          >
                            {savingScripts && (
                              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                            )}
                            Save Scripts
                          </Button>
                        </div>
                      </div>
                    </>
                  ) : null}
                </>
              )}
            </div>
          </SettingsSection>

          {/* Sticky Save Button for Project Name */}
          {hasUnsavedProjectChanges && (
            <div className="sticky bottom-0 z-10 bg-background/80 backdrop-blur-sm border-t py-4">
              <div className="flex items-center justify-between">
                <Text variant="secondary" size="sm">
                  {t('settings.projects.save.unsavedChanges')}
                </Text>
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    onClick={handleDiscard}
                    disabled={saving}
                  >
                    {t('settings.projects.save.discard')}
                  </Button>
                  <Button onClick={handleSave} disabled={saving}>
                    {saving && (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    )}
                    {t('settings.projects.save.button')}
                  </Button>
                </div>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
