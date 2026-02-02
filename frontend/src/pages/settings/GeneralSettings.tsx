import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, merge, isEqual } from 'lodash';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Checkbox } from '@/components/ui/checkbox';
import { ChevronDown, ChevronRight, Folder, Loader2, Volume2 } from 'lucide-react';
import {
  AvailableSoundsResponse,
  DEFAULT_COMMIT_MESSAGE_PROMPT,
  DEFAULT_PR_DESCRIPTION_PROMPT,
  EditorType,
  SoundFile,
  ThemeMode,
  UiLanguage,
} from 'shared/types';
import { getLanguageOptions } from '@/i18n/languages';

import { toPrettyCase } from '@/utils/string';
import { useEditorAvailability } from '@/hooks/useEditorAvailability';
import { EditorAvailabilityIndicator } from '@/components/EditorAvailabilityIndicator';
import { useTheme } from '@/components/ThemeProvider';
import { useUserSystem } from '@/components/ConfigProvider';
import { TagManager } from '@/components/TagManager';
import { FolderPickerDialog } from '@/components/dialogs/shared/FolderPickerDialog';
import ExecutorProfileSelector from '@/components/settings/ExecutorProfileSelector';
import { CustomEditorsList } from '@/components/settings/CustomEditorsList';
import { ServerModeSettings } from '@/components/settings/ServerModeSettings';
import { soundsApi } from '@/lib/api';
import { playSound } from '@/lib/soundUtils';
import { SettingsSection } from '@/components/settings/SettingsSection';
import { SettingsField } from '@/components/settings/SettingsField';
import { Text } from '@/components/ui/text';
import { SkeletonForm } from '@/components/ui/loading-states';

export function GeneralSettings() {
  const { t } = useTranslation(['settings', 'common']);

  // Get language options with proper display names
  const languageOptions = getLanguageOptions(
    t('language.browserDefault', {
      ns: 'common',
      defaultValue: 'Browser Default',
    })
  );
  const {
    config,
    loading,
    profiles,
    updateAndSaveConfig, // Use this on Save
  } = useUserSystem();

  // Draft state management
  const [draft, setDraft] = useState(() => (config ? cloneDeep(config) : null));
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const [branchPrefixError, setBranchPrefixError] = useState<string | null>(
    null
  );
  const [availableSounds, setAvailableSounds] = useState<AvailableSoundsResponse | null>(null);
  const [soundsLoading, setSoundsLoading] = useState(false);
  const [backupAdvancedOpen, setBackupAdvancedOpen] = useState(false);
  const { setTheme } = useTheme();

  // Check editor availability when draft editor changes
  const editorAvailability = useEditorAvailability(draft?.editor.editor_type);

  // Fetch available sounds on mount
  useEffect(() => {
    const fetchSounds = async () => {
      setSoundsLoading(true);
      try {
        const sounds = await soundsApi.list();
        setAvailableSounds(sounds);
      } catch (err) {
        console.error('Failed to fetch sounds:', err);
      } finally {
        setSoundsLoading(false);
      }
    };
    fetchSounds();
  }, []);

  const validateBranchPrefix = useCallback(
    (prefix: string): string | null => {
      if (!prefix) return null; // empty allowed
      if (prefix.includes('/'))
        return t('settings.general.git.branchPrefix.errors.slash');
      if (prefix.startsWith('.'))
        return t('settings.general.git.branchPrefix.errors.startsWithDot');
      if (prefix.endsWith('.') || prefix.endsWith('.lock'))
        return t('settings.general.git.branchPrefix.errors.endsWithDot');
      if (prefix.includes('..') || prefix.includes('@{'))
        return t('settings.general.git.branchPrefix.errors.invalidSequence');
      if (/[ \t~^:?*[\\]/.test(prefix))
        return t('settings.general.git.branchPrefix.errors.invalidChars');
      // Control chars check
      for (let i = 0; i < prefix.length; i++) {
        const code = prefix.charCodeAt(i);
        if (code < 0x20 || code === 0x7f)
          return t('settings.general.git.branchPrefix.errors.controlChars');
      }
      return null;
    },
    [t]
  );

  // When config loads or changes externally, update draft only if not dirty
  useEffect(() => {
    if (!config) return;
    if (!dirty) {
      setDraft(cloneDeep(config));
    }
  }, [config, dirty]);

  // Check for unsaved changes
  const hasUnsavedChanges = useMemo(() => {
    if (!draft || !config) return false;
    return !isEqual(draft, config);
  }, [draft, config]);

  // Generic draft update helper
  const updateDraft = useCallback(
    (patch: Partial<typeof config>) => {
      setDraft((prev: typeof config) => {
        if (!prev) return prev;
        const next = merge({}, prev, patch);
        // Mark dirty if changed
        if (!isEqual(next, config)) {
          setDirty(true);
        }
        return next;
      });
    },
    [config]
  );

  // Optional: warn on tab close/navigation with unsaved changes
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

  // Get the current sound identifier based on draft state
  const getCurrentSoundIdentifier = (): string | null => {
    if (!draft) return null;
    if (draft.notifications.custom_sound_path) {
      return `custom:${draft.notifications.custom_sound_path}`;
    }
    return `bundled:${draft.notifications.sound_file}`;
  };

  // Handle sound selection from dropdown
  const handleSoundSelect = (identifier: string) => {
    if (identifier.startsWith('custom:')) {
      const filename = identifier.slice('custom:'.length);
      updateDraft({
        notifications: {
          ...draft!.notifications,
          custom_sound_path: filename,
        },
      });
    } else if (identifier.startsWith('bundled:')) {
      const soundFile = identifier.slice('bundled:'.length) as SoundFile;
      updateDraft({
        notifications: {
          ...draft!.notifications,
          sound_file: soundFile,
          custom_sound_path: null,
        },
      });
    }
  };

  const handleSave = async () => {
    if (!draft) return;

    setSaving(true);
    setError(null);
    setSuccess(false);

    try {
      await updateAndSaveConfig(draft); // Atomically apply + persist
      setTheme(draft.theme);
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.general.save.error'));
      console.error('Error saving config:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleDiscard = () => {
    if (!config) return;
    setDraft(cloneDeep(config));
    setDirty(false);
  };

  const resetDisclaimer = async () => {
    if (!config) return;
    updateAndSaveConfig({ disclaimer_acknowledged: false });
  };

  const resetOnboarding = async () => {
    if (!config) return;
    updateAndSaveConfig({ onboarding_acknowledged: false });
  };

  if (loading) {
    return (
      <div className="space-y-6">
        <SkeletonForm fields={4} />
        <SkeletonForm fields={3} />
        <SkeletonForm fields={2} />
      </div>
    );
  }

  if (!config) {
    return (
      <div className="py-8">
        <Alert variant="destructive">
          <AlertDescription>{t('settings.general.loadError')}</AlertDescription>
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
            {t('settings.general.save.success')}
          </AlertDescription>
        </Alert>
      )}

      <ServerModeSettings />

      {/* Editor Section - expanded by default */}
      <SettingsSection
        id="general-editor"
        title={t('settings.general.editor.title')}
        description={t('settings.general.editor.description')}
        defaultOpen={true}
      >
        <SettingsField
          label={t('settings.general.editor.type.label')}
          htmlFor="editor-type"
          description={t('settings.general.editor.type.helper')}
        >
          <Select
            value={draft?.editor.editor_type}
            onValueChange={(value: EditorType) =>
              updateDraft({
                editor: { ...draft!.editor, editor_type: value },
              })
            }
          >
            <SelectTrigger id="editor-type">
              <SelectValue
                placeholder={t('settings.general.editor.type.placeholder')}
              />
            </SelectTrigger>
            <SelectContent>
              {Object.values(EditorType).map((editor) => (
                <SelectItem key={editor} value={editor}>
                  {toPrettyCase(editor)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {/* Editor availability status indicator */}
          {draft?.editor.editor_type !== EditorType.CUSTOM && (
            <EditorAvailabilityIndicator availability={editorAvailability} />
          )}
        </SettingsField>

        {draft?.editor.editor_type === EditorType.CUSTOM && (
          <SettingsField
            label={t('settings.general.editor.customCommand.label')}
            htmlFor="custom-command"
            description={t('settings.general.editor.customCommand.helper')}
          >
            <Input
              id="custom-command"
              placeholder={t(
                'settings.general.editor.customCommand.placeholder'
              )}
              value={draft?.editor.custom_command || ''}
              onChange={(e) =>
                updateDraft({
                  editor: {
                    ...draft!.editor,
                    custom_command: e.target.value || null,
                  },
                })
              }
            />
          </SettingsField>
        )}

        {(draft?.editor.editor_type === EditorType.VS_CODE ||
          draft?.editor.editor_type === EditorType.CURSOR ||
          draft?.editor.editor_type === EditorType.WINDSURF) && (
          <>
            <SettingsField
              label={t('settings.general.editor.remoteSsh.host.label')}
              htmlFor="remote-ssh-host"
              description={t('settings.general.editor.remoteSsh.host.helper')}
            >
              <Input
                id="remote-ssh-host"
                placeholder={t(
                  'settings.general.editor.remoteSsh.host.placeholder'
                )}
                value={draft?.editor.remote_ssh_host || ''}
                onChange={(e) =>
                  updateDraft({
                    editor: {
                      ...draft!.editor,
                      remote_ssh_host: e.target.value || null,
                    },
                  })
                }
              />
            </SettingsField>

            {draft?.editor.remote_ssh_host && (
              <SettingsField
                label={t('settings.general.editor.remoteSsh.user.label')}
                htmlFor="remote-ssh-user"
                description={t('settings.general.editor.remoteSsh.user.helper')}
              >
                <Input
                  id="remote-ssh-user"
                  placeholder={t(
                    'settings.general.editor.remoteSsh.user.placeholder'
                  )}
                  value={draft?.editor.remote_ssh_user || ''}
                  onChange={(e) =>
                    updateDraft({
                      editor: {
                        ...draft!.editor,
                        remote_ssh_user: e.target.value || null,
                      },
                    })
                  }
                />
              </SettingsField>
            )}
          </>
        )}

        {/* Custom Editors subsection */}
        <div className="pt-4 border-t">
          <Text size="sm" className="font-medium mb-3">
            {t('settings.general.customEditors.title')}
          </Text>
          <Text variant="secondary" size="sm" as="p" className="mb-4">
            {t('settings.general.customEditors.description')}
          </Text>
          <CustomEditorsList />
        </div>
      </SettingsSection>

      {/* Git Section */}
      <SettingsSection
        id="general-git"
        title={t('settings.general.git.title')}
        description={t('settings.general.git.description')}
      >
        <SettingsField
          label={t('settings.general.git.branchPrefix.label')}
          htmlFor="git-branch-prefix"
          error={branchPrefixError}
          description={
            <>
              {t('settings.general.git.branchPrefix.helper')}{' '}
              {draft?.git_branch_prefix ? (
                <>
                  {t('settings.general.git.branchPrefix.preview')}{' '}
                  <code className="text-xs bg-muted px-1 py-0.5 rounded">
                    {t('settings.general.git.branchPrefix.previewWithPrefix', {
                      prefix: draft.git_branch_prefix,
                    })}
                  </code>
                </>
              ) : (
                <>
                  {t('settings.general.git.branchPrefix.preview')}{' '}
                  <code className="text-xs bg-muted px-1 py-0.5 rounded">
                    {t('settings.general.git.branchPrefix.previewNoPrefix')}
                  </code>
                </>
              )}
            </>
          }
        >
          <Input
            id="git-branch-prefix"
            type="text"
            placeholder={t('settings.general.git.branchPrefix.placeholder')}
            value={draft?.git_branch_prefix ?? ''}
            onChange={(e) => {
              const value = e.target.value.trim();
              updateDraft({ git_branch_prefix: value });
              setBranchPrefixError(validateBranchPrefix(value));
            }}
            aria-invalid={!!branchPrefixError}
            className={branchPrefixError ? 'border-destructive' : undefined}
          />
        </SettingsField>

        <SettingsField
          label={t('settings.general.git.defaultCloneDirectory.label')}
          htmlFor="default-clone-directory"
          description={t('settings.general.git.defaultCloneDirectory.helper')}
        >
          <div className="flex space-x-2">
            <Input
              id="default-clone-directory"
              type="text"
              placeholder={t(
                'settings.general.git.defaultCloneDirectory.placeholder'
              )}
              value={draft?.default_clone_directory ?? ''}
              onChange={(e) =>
                updateDraft({ default_clone_directory: e.target.value || null })
              }
              className="flex-1"
            />
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={async () => {
                const result = await FolderPickerDialog.show({
                  value: draft?.default_clone_directory ?? '',
                  title: t(
                    'settings.general.git.defaultCloneDirectory.browseTitle'
                  ),
                  description: t(
                    'settings.general.git.defaultCloneDirectory.browseDescription'
                  ),
                });
                if (result) {
                  updateDraft({ default_clone_directory: result });
                }
              }}
            >
              <Folder className="h-4 w-4" />
            </Button>
          </div>
        </SettingsField>

        {/* Pull Requests subsection */}
        <div className="pt-4 border-t space-y-4">
          <div>
            <Text size="sm" className="font-medium">
              {t('settings.general.pullRequests.title')}
            </Text>
            <Text variant="secondary" size="sm" as="p">
              {t('settings.general.pullRequests.description')}
            </Text>
          </div>

          <SettingsField
            label={t('settings.general.pullRequests.autoDescription.label')}
            htmlFor="pr-auto-description"
            description={t('settings.general.pullRequests.autoDescription.helper')}
            layout="horizontal"
          >
            <Checkbox
              id="pr-auto-description"
              checked={draft?.pr_auto_description_enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({ pr_auto_description_enabled: checked })
              }
            />
          </SettingsField>

          <SettingsField
            label={t('settings.general.pullRequests.customPrompt.useCustom')}
            htmlFor="use-custom-prompt"
            layout="horizontal"
          >
            <Checkbox
              id="use-custom-prompt"
              checked={draft?.pr_auto_description_prompt != null}
              onCheckedChange={(checked: boolean) => {
                if (checked) {
                  updateDraft({
                    pr_auto_description_prompt: DEFAULT_PR_DESCRIPTION_PROMPT,
                  });
                } else {
                  updateDraft({ pr_auto_description_prompt: null });
                }
              }}
            />
          </SettingsField>

          <SettingsField
            description={t('settings.general.pullRequests.customPrompt.helper')}
          >
            <textarea
              id="pr-custom-prompt"
              className={`flex min-h-[100px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 ${
                draft?.pr_auto_description_prompt == null
                  ? 'opacity-50 cursor-not-allowed'
                  : ''
              }`}
              value={
                draft?.pr_auto_description_prompt ??
                DEFAULT_PR_DESCRIPTION_PROMPT
              }
              disabled={draft?.pr_auto_description_prompt == null}
              onChange={(e) =>
                updateDraft({
                  pr_auto_description_prompt: e.target.value,
                })
              }
            />
          </SettingsField>
        </div>

        {/* Commit Message subsection */}
        <div className="pt-4 border-t space-y-4">
          <div>
            <Text size="sm" className="font-medium">
              {t('settings.general.commitMessage.title')}
            </Text>
            <Text variant="secondary" size="sm" as="p">
              {t('settings.general.commitMessage.description')}
            </Text>
          </div>

          <SettingsField
            label={t('settings.general.commitMessage.autoGenerate.label')}
            htmlFor="commit-message-auto-generate"
            description={t('settings.general.commitMessage.autoGenerate.helper')}
            layout="horizontal"
          >
            <Checkbox
              id="commit-message-auto-generate"
              checked={draft?.commit_message_auto_generate_enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({ commit_message_auto_generate_enabled: checked })
              }
            />
          </SettingsField>

          <SettingsField
            label={t('settings.general.commitMessage.customPrompt.useCustom')}
            htmlFor="commit-message-use-custom-prompt"
            layout="horizontal"
          >
            <Checkbox
              id="commit-message-use-custom-prompt"
              checked={draft?.commit_message_prompt != null}
              onCheckedChange={(checked: boolean) => {
                if (checked) {
                  updateDraft({
                    commit_message_prompt: DEFAULT_COMMIT_MESSAGE_PROMPT,
                  });
                } else {
                  updateDraft({ commit_message_prompt: null });
                }
              }}
            />
          </SettingsField>

          <SettingsField
            description={t('settings.general.commitMessage.customPrompt.helper')}
          >
            <textarea
              id="commit-message-custom-prompt"
              className={`flex min-h-[100px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 ${
                draft?.commit_message_prompt == null
                  ? 'opacity-50 cursor-not-allowed'
                  : ''
              }`}
              value={
                draft?.commit_message_prompt ??
                DEFAULT_COMMIT_MESSAGE_PROMPT
              }
              disabled={draft?.commit_message_prompt == null}
              onChange={(e) =>
                updateDraft({
                  commit_message_prompt: e.target.value,
                })
              }
            />
          </SettingsField>

          <SettingsField
            label={t('settings.general.commitMessage.executorProfile.label')}
            description={t('settings.general.commitMessage.executorProfile.helper')}
          >
            <ExecutorProfileSelector
              profiles={profiles}
              selectedProfile={draft?.commit_message_executor_profile ?? null}
              onProfileSelect={(profile) =>
                updateDraft({ commit_message_executor_profile: profile })
              }
              showLabel={false}
            />
          </SettingsField>
        </div>
      </SettingsSection>

      {/* Backup Section */}
      <SettingsSection
        id="general-backup"
        title={t('settings.general.backup.title')}
        description={t('settings.general.backup.description')}
      >
        <SettingsField
          label={t('settings.general.backup.enabled.label')}
          htmlFor="backup-enabled"
          description={t('settings.general.backup.enabled.helper')}
          layout="horizontal"
        >
          <Checkbox
            id="backup-enabled"
            checked={draft?.backup.enabled ?? false}
            onCheckedChange={(checked: boolean) =>
              updateDraft({
                backup: { ...draft!.backup, enabled: checked },
              })
            }
          />
        </SettingsField>

        {draft?.backup.enabled && (
          <>
            <SettingsField
              label={t('settings.general.backup.interval.label')}
              htmlFor="backup-interval"
              description={t('settings.general.backup.interval.helper')}
              indent
            >
              <Input
                id="backup-interval"
                type="number"
                min="1"
                placeholder={t('settings.general.backup.interval.placeholder')}
                value={draft?.backup.interval_hours ?? 24}
                onChange={(e) => {
                  const value = parseInt(e.target.value, 10) || 1;
                  updateDraft({
                    backup: {
                      ...draft!.backup,
                      interval_hours: Math.max(1, value),
                    },
                  });
                }}
                className="w-32"
              />
            </SettingsField>

            <div className="ml-6">
              <button
                type="button"
                onClick={() => setBackupAdvancedOpen(!backupAdvancedOpen)}
                className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition-colors"
              >
                {backupAdvancedOpen ? (
                  <ChevronDown className="h-4 w-4" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
                {t('settings.general.backup.advanced.title')}
              </button>

              {backupAdvancedOpen && (
                <div className="mt-4 space-y-4 pl-5 border-l-2 border-muted">
                  <Text variant="secondary" size="sm" as="p">
                    {t('settings.general.backup.advanced.description')}
                  </Text>

                  <SettingsField
                    label={t('settings.general.backup.advanced.hoursAll.label')}
                    htmlFor="backup-hours-all"
                    description={t('settings.general.backup.advanced.hoursAll.helper')}
                  >
                    <Input
                      id="backup-hours-all"
                      type="number"
                      min="1"
                      value={draft?.backup.retention_hours_all ?? 48}
                      onChange={(e) => {
                        const value = parseInt(e.target.value, 10) || 1;
                        updateDraft({
                          backup: {
                            ...draft!.backup,
                            retention_hours_all: Math.max(1, value),
                          },
                        });
                      }}
                      className="w-32"
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.general.backup.advanced.dailyDays.label')}
                    htmlFor="backup-daily-days"
                    description={t('settings.general.backup.advanced.dailyDays.helper')}
                  >
                    <Input
                      id="backup-daily-days"
                      type="number"
                      min="0"
                      value={draft?.backup.retention_daily_days ?? 7}
                      onChange={(e) => {
                        const value = parseInt(e.target.value, 10) || 0;
                        updateDraft({
                          backup: {
                            ...draft!.backup,
                            retention_daily_days: Math.max(0, value),
                          },
                        });
                      }}
                      className="w-32"
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.general.backup.advanced.weeklyWeeks.label')}
                    htmlFor="backup-weekly-weeks"
                    description={t('settings.general.backup.advanced.weeklyWeeks.helper')}
                  >
                    <Input
                      id="backup-weekly-weeks"
                      type="number"
                      min="0"
                      value={draft?.backup.retention_weekly_weeks ?? 4}
                      onChange={(e) => {
                        const value = parseInt(e.target.value, 10) || 0;
                        updateDraft({
                          backup: {
                            ...draft!.backup,
                            retention_weekly_weeks: Math.max(0, value),
                          },
                        });
                      }}
                      className="w-32"
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.general.backup.advanced.monthlyMonths.label')}
                    htmlFor="backup-monthly-months"
                    description={t('settings.general.backup.advanced.monthlyMonths.helper')}
                  >
                    <Input
                      id="backup-monthly-months"
                      type="number"
                      min="0"
                      value={draft?.backup.retention_monthly_months ?? 12}
                      onChange={(e) => {
                        const value = parseInt(e.target.value, 10) || 0;
                        updateDraft({
                          backup: {
                            ...draft!.backup,
                            retention_monthly_months: Math.max(0, value),
                          },
                        });
                      }}
                      className="w-32"
                    />
                  </SettingsField>
                </div>
              )}
            </div>
          </>
        )}
      </SettingsSection>

      {/* UI & Theme Section */}
      <SettingsSection
        id="general-ui"
        title={t('settings.general.appearance.title')}
        description={t('settings.general.appearance.description')}
      >
        <SettingsField
          label={t('settings.general.appearance.theme.label')}
          htmlFor="theme"
          description={t('settings.general.appearance.theme.helper')}
        >
          <Select
            value={draft?.theme}
            onValueChange={(value: ThemeMode) =>
              updateDraft({ theme: value })
            }
          >
            <SelectTrigger id="theme">
              <SelectValue
                placeholder={t(
                  'settings.general.appearance.theme.placeholder'
                )}
              />
            </SelectTrigger>
            <SelectContent>
              {Object.values(ThemeMode).map((theme) => (
                <SelectItem key={theme} value={theme}>
                  {toPrettyCase(theme)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </SettingsField>

        <SettingsField
          label={t('settings.general.appearance.language.label')}
          htmlFor="language"
          description={t('settings.general.appearance.language.helper')}
        >
          <Select
            value={draft?.language}
            onValueChange={(value: UiLanguage) =>
              updateDraft({ language: value })
            }
          >
            <SelectTrigger id="language">
              <SelectValue
                placeholder={t(
                  'settings.general.appearance.language.placeholder'
                )}
              />
            </SelectTrigger>
            <SelectContent>
              {languageOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </SettingsField>
      </SettingsSection>

      {/* Notifications Section */}
      <SettingsSection
        id="general-notifications"
        title={t('settings.general.notifications.title')}
        description={t('settings.general.notifications.description')}
      >
        <SettingsField
          label={t('settings.general.notifications.sound.label')}
          htmlFor="sound-enabled"
          description={t('settings.general.notifications.sound.helper')}
          layout="horizontal"
        >
          <Checkbox
            id="sound-enabled"
            checked={draft?.notifications.sound_enabled}
            onCheckedChange={(checked: boolean) =>
              updateDraft({
                notifications: {
                  ...draft!.notifications,
                  sound_enabled: checked,
                },
              })
            }
          />
        </SettingsField>

        {draft?.notifications.sound_enabled && (
          <>
            <SettingsField
              label={t('settings.general.notifications.sound.fileLabel')}
              htmlFor="sound-file"
              description={t('settings.general.notifications.sound.fileHelper')}
              indent
            >
              <div className="flex gap-2">
                <Select
                  value={getCurrentSoundIdentifier() ?? undefined}
                  onValueChange={handleSoundSelect}
                  disabled={soundsLoading}
                >
                  <SelectTrigger id="sound-file" className="flex-1">
                    {soundsLoading ? (
                      <span className="flex items-center gap-2">
                        <Loader2 className="h-4 w-4 animate-spin" />
                        {t('common:loading', { defaultValue: 'Loading...' })}
                      </span>
                    ) : (
                      <SelectValue
                        placeholder={t(
                          'settings.general.notifications.sound.filePlaceholder'
                        )}
                      />
                    )}
                  </SelectTrigger>
                  <SelectContent>
                    {availableSounds && (
                      <>
                        <SelectGroup>
                          <SelectLabel>
                            {t('settings.general.notifications.sound.bundledSounds', { defaultValue: 'Bundled Sounds' })}
                          </SelectLabel>
                          {availableSounds.bundled.map((sound) => (
                            <SelectItem key={sound.identifier} value={sound.identifier}>
                              {sound.display_name}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                        {availableSounds.custom.length > 0 ? (
                          <SelectGroup>
                            <SelectLabel>
                              {t('settings.general.notifications.sound.customSounds', { defaultValue: 'Custom Sounds' })}
                            </SelectLabel>
                            {availableSounds.custom.map((sound) => (
                              <SelectItem key={`custom:${sound.filename}`} value={`custom:${sound.filename}`}>
                                {toPrettyCase(sound.filename.replace(/\.(wav|mp3)$/i, ''))}
                              </SelectItem>
                            ))}
                          </SelectGroup>
                        ) : (
                          <div className="py-2 px-3 text-sm text-muted-foreground">
                            {t('settings.general.notifications.sound.noCustomSounds', {
                              defaultValue: 'No custom sounds found. Add .wav or .mp3 files to ~/.vibe-kanban/alerts/'
                            })}
                          </div>
                        )}
                      </>
                    )}
                  </SelectContent>
                </Select>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    const identifier = getCurrentSoundIdentifier();
                    if (identifier) playSound(identifier);
                  }}
                  className="px-3"
                  disabled={!getCurrentSoundIdentifier()}
                >
                  <Volume2 className="h-4 w-4" />
                </Button>
              </div>
            </SettingsField>

            <SettingsField
              label="Error Sound"
              htmlFor="error-sound-file"
              description="Sound played when a task fails or an error occurs"
              indent
            >
              <div className="flex gap-2">
                <Select
                  value={draft.notifications.error_sound_file}
                  onValueChange={(value: SoundFile) =>
                    updateDraft({
                      notifications: {
                        ...draft.notifications,
                        error_sound_file: value,
                      },
                    })
                  }
                >
                  <SelectTrigger id="error-sound-file" className="flex-1">
                    <SelectValue placeholder="Select error sound" />
                  </SelectTrigger>
                  <SelectContent>
                    {Object.values(SoundFile).map((soundFile) => (
                      <SelectItem key={soundFile} value={soundFile}>
                        {toPrettyCase(soundFile)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => playSound(`bundled:${draft.notifications.error_sound_file}`)}
                  className="px-3"
                >
                  <Volume2 className="h-4 w-4" />
                </Button>
              </div>
            </SettingsField>
          </>
        )}

        <SettingsField
          label={t('settings.general.notifications.push.label')}
          htmlFor="push-notifications"
          description={t('settings.general.notifications.push.helper')}
          layout="horizontal"
        >
          <Checkbox
            id="push-notifications"
            checked={draft?.notifications.push_enabled}
            onCheckedChange={(checked: boolean) =>
              updateDraft({
                notifications: {
                  ...draft!.notifications,
                  push_enabled: checked,
                },
              })
            }
          />
        </SettingsField>
      </SettingsSection>

      {/* Advanced Section */}
      <SettingsSection
        id="general-advanced"
        title={t('settings.general.concurrency.title')}
        description="Advanced settings including concurrency, observability, privacy, and more"
      >
        {/* Concurrency */}
        <SettingsField
          label={t('settings.general.concurrency.maxAgents.label')}
          htmlFor="max-concurrent-agents"
          description={t('settings.general.concurrency.maxAgents.helper')}
        >
          <Input
            id="max-concurrent-agents"
            type="number"
            min="0"
            placeholder={t('settings.general.concurrency.maxAgents.placeholder')}
            value={draft?.max_concurrent_agents ?? 0}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10) || 0;
              updateDraft({ max_concurrent_agents: Math.max(0, value) });
            }}
            className="w-32"
          />
        </SettingsField>

        {/* Observability */}
        <div className="pt-4 border-t space-y-4">
          <div>
            <Text size="sm" className="font-medium">
              {t('settings.general.observability.title')}
            </Text>
            <Text variant="secondary" size="sm" as="p">
              {t('settings.general.observability.description')}
            </Text>
          </div>

          <SettingsField
            label={t('settings.general.observability.langfuse.enabled.label')}
            htmlFor="langfuse-enabled"
            description={t('settings.general.observability.langfuse.enabled.helper')}
            layout="horizontal"
          >
            <Checkbox
              id="langfuse-enabled"
              checked={draft?.langfuse_enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({ langfuse_enabled: checked })
              }
            />
          </SettingsField>

          {draft?.langfuse_enabled && (
            <>
              <SettingsField
                label={t('settings.general.observability.langfuse.publicKey.label')}
                htmlFor="langfuse-public-key"
                description={t('settings.general.observability.langfuse.publicKey.helper')}
                indent
              >
                <Input
                  id="langfuse-public-key"
                  type="text"
                  placeholder={t('settings.general.observability.langfuse.publicKey.placeholder')}
                  value={draft?.langfuse_public_key ?? ''}
                  onChange={(e) =>
                    updateDraft({ langfuse_public_key: e.target.value || null })
                  }
                />
              </SettingsField>

              <SettingsField
                label={t('settings.general.observability.langfuse.secretKey.label')}
                htmlFor="langfuse-secret-key"
                description={t('settings.general.observability.langfuse.secretKey.helper')}
                indent
              >
                <Input
                  id="langfuse-secret-key"
                  type="password"
                  placeholder={t('settings.general.observability.langfuse.secretKey.placeholder')}
                  value={draft?.langfuse_secret_key ?? ''}
                  onChange={(e) =>
                    updateDraft({ langfuse_secret_key: e.target.value || null })
                  }
                />
              </SettingsField>

              <SettingsField
                label={t('settings.general.observability.langfuse.host.label')}
                htmlFor="langfuse-host"
                description={t('settings.general.observability.langfuse.host.helper')}
                indent
              >
                <Input
                  id="langfuse-host"
                  type="text"
                  placeholder="https://cloud.langfuse.com"
                  value={draft?.langfuse_host ?? ''}
                  onChange={(e) =>
                    updateDraft({ langfuse_host: e.target.value || null })
                  }
                />
              </SettingsField>
            </>
          )}
        </div>

        {/* Privacy */}
        <div className="pt-4 border-t space-y-4">
          <div>
            <Text size="sm" className="font-medium">
              {t('settings.general.privacy.title')}
            </Text>
            <Text variant="secondary" size="sm" as="p">
              {t('settings.general.privacy.description')}
            </Text>
          </div>

          <SettingsField
            label={t('settings.general.privacy.telemetry.label')}
            htmlFor="analytics-enabled"
            description={t('settings.general.privacy.telemetry.helper')}
            layout="horizontal"
          >
            <Checkbox
              id="analytics-enabled"
              checked={draft?.analytics_enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({ analytics_enabled: checked })
              }
            />
          </SettingsField>
        </div>

        {/* Task Templates */}
        <div className="pt-4 border-t space-y-4">
          <div>
            <Text size="sm" className="font-medium">
              {t('settings.general.taskTemplates.title')}
            </Text>
            <Text variant="secondary" size="sm" as="p">
              {t('settings.general.taskTemplates.description')}
            </Text>
          </div>
          <TagManager />
        </div>

        {/* Safety */}
        <div className="pt-4 border-t space-y-4">
          <div>
            <Text size="sm" className="font-medium">
              {t('settings.general.safety.title')}
            </Text>
            <Text variant="secondary" size="sm" as="p">
              {t('settings.general.safety.description')}
            </Text>
          </div>

          <div className="flex items-center justify-between">
            <div>
              <Text size="sm" className="font-medium">
                {t('settings.general.safety.disclaimer.title')}
              </Text>
              <Text variant="secondary" size="sm" as="p">
                {t('settings.general.safety.disclaimer.description')}
              </Text>
            </div>
            <Button variant="outline" onClick={resetDisclaimer}>
              {t('settings.general.safety.disclaimer.button')}
            </Button>
          </div>

          <div className="flex items-center justify-between">
            <div>
              <Text size="sm" className="font-medium">
                {t('settings.general.safety.onboarding.title')}
              </Text>
              <Text variant="secondary" size="sm" as="p">
                {t('settings.general.safety.onboarding.description')}
              </Text>
            </div>
            <Button variant="outline" onClick={resetOnboarding}>
              {t('settings.general.safety.onboarding.button')}
            </Button>
          </div>
        </div>
      </SettingsSection>

      {/* Sticky Save Button */}
      <div className="sticky bottom-0 z-10 bg-background/80 backdrop-blur-sm border-t py-4">
        <div className="flex items-center justify-between">
          {hasUnsavedChanges ? (
            <Text variant="secondary" size="sm">
              {t('settings.general.save.unsavedChanges')}
            </Text>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={handleDiscard}
              disabled={!hasUnsavedChanges || saving}
            >
              {t('settings.general.save.discard')}
            </Button>
            <Button
              onClick={handleSave}
              disabled={!hasUnsavedChanges || saving || !!branchPrefixError}
            >
              {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t('settings.general.save.button')}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
