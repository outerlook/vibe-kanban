import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, merge, isEqual } from 'lodash';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
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
import { Label } from '@/components/ui/label';
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
import { soundsApi } from '@/lib/api';

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

  const playSound = async (identifier: string) => {
    const audio = new Audio(`/api/sounds/${identifier}`);
    try {
      await audio.play();
    } catch (err) {
      console.error('Failed to play sound:', err);
    }
  };

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
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-8 w-8 animate-spin" />
        <span className="ml-2">{t('settings.general.loading')}</span>
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

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.appearance.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.appearance.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="theme">
              {t('settings.general.appearance.theme.label')}
            </Label>
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
            <p className="text-sm text-muted-foreground">
              {t('settings.general.appearance.theme.helper')}
            </p>
          </div>

          <div className="space-y-2">
            <Label htmlFor="language">
              {t('settings.general.appearance.language.label')}
            </Label>
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
            <p className="text-sm text-muted-foreground">
              {t('settings.general.appearance.language.helper')}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.editor.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.editor.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="editor-type">
              {t('settings.general.editor.type.label')}
            </Label>
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

            <p className="text-sm text-muted-foreground">
              {t('settings.general.editor.type.helper')}
            </p>
          </div>

          {draft?.editor.editor_type === EditorType.CUSTOM && (
            <div className="space-y-2">
              <Label htmlFor="custom-command">
                {t('settings.general.editor.customCommand.label')}
              </Label>
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
              <p className="text-sm text-muted-foreground">
                {t('settings.general.editor.customCommand.helper')}
              </p>
            </div>
          )}

          {(draft?.editor.editor_type === EditorType.VS_CODE ||
            draft?.editor.editor_type === EditorType.CURSOR ||
            draft?.editor.editor_type === EditorType.WINDSURF) && (
            <>
              <div className="space-y-2">
                <Label htmlFor="remote-ssh-host">
                  {t('settings.general.editor.remoteSsh.host.label')}
                </Label>
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
                <p className="text-sm text-muted-foreground">
                  {t('settings.general.editor.remoteSsh.host.helper')}
                </p>
              </div>

              {draft?.editor.remote_ssh_host && (
                <div className="space-y-2">
                  <Label htmlFor="remote-ssh-user">
                    {t('settings.general.editor.remoteSsh.user.label')}
                  </Label>
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
                  <p className="text-sm text-muted-foreground">
                    {t('settings.general.editor.remoteSsh.user.helper')}
                  </p>
                </div>
              )}
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.customEditors.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.customEditors.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <CustomEditorsList />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.git.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.git.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="git-branch-prefix">
              {t('settings.general.git.branchPrefix.label')}
            </Label>
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
            {branchPrefixError && (
              <p className="text-sm text-destructive">{branchPrefixError}</p>
            )}
            <p className="text-sm text-muted-foreground">
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
            </p>
          </div>

          <div className="space-y-2">
            <Label htmlFor="default-clone-directory">
              {t('settings.general.git.defaultCloneDirectory.label')}
            </Label>
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
            <p className="text-sm text-muted-foreground">
              {t('settings.general.git.defaultCloneDirectory.helper')}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.pullRequests.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.pullRequests.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center space-x-2">
            <Checkbox
              id="pr-auto-description"
              checked={draft?.pr_auto_description_enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({ pr_auto_description_enabled: checked })
              }
            />
            <div className="space-y-0.5">
              <Label htmlFor="pr-auto-description" className="cursor-pointer">
                {t('settings.general.pullRequests.autoDescription.label')}
              </Label>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.pullRequests.autoDescription.helper')}
              </p>
            </div>
          </div>
          <div className="flex items-center space-x-2">
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
            <Label htmlFor="use-custom-prompt" className="cursor-pointer">
              {t('settings.general.pullRequests.customPrompt.useCustom')}
            </Label>
          </div>
          <div className="space-y-2">
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
            <p className="text-sm text-muted-foreground">
              {t('settings.general.pullRequests.customPrompt.helper')}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.commitMessage.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.commitMessage.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center space-x-2">
            <Checkbox
              id="commit-message-auto-generate"
              checked={draft?.commit_message_auto_generate_enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({ commit_message_auto_generate_enabled: checked })
              }
            />
            <div className="space-y-0.5">
              <Label htmlFor="commit-message-auto-generate" className="cursor-pointer">
                {t('settings.general.commitMessage.autoGenerate.label')}
              </Label>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.commitMessage.autoGenerate.helper')}
              </p>
            </div>
          </div>
          <div className="flex items-center space-x-2">
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
            <Label htmlFor="commit-message-use-custom-prompt" className="cursor-pointer">
              {t('settings.general.commitMessage.customPrompt.useCustom')}
            </Label>
          </div>
          <div className="space-y-2">
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
            <p className="text-sm text-muted-foreground">
              {t('settings.general.commitMessage.customPrompt.helper')}
            </p>
          </div>
          <div className="space-y-2">
            <Label>{t('settings.general.commitMessage.executorProfile.label')}</Label>
            <ExecutorProfileSelector
              profiles={profiles}
              selectedProfile={draft?.commit_message_executor_profile ?? null}
              onProfileSelect={(profile) =>
                updateDraft({ commit_message_executor_profile: profile })
              }
              showLabel={false}
            />
            <p className="text-sm text-muted-foreground">
              {t('settings.general.commitMessage.executorProfile.helper')}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.notifications.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.notifications.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center space-x-2">
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
            <div className="space-y-0.5">
              <Label htmlFor="sound-enabled" className="cursor-pointer">
                {t('settings.general.notifications.sound.label')}
              </Label>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.notifications.sound.helper')}
              </p>
            </div>
          </div>
          {draft?.notifications.sound_enabled && (
            <>
              <div className="ml-6 space-y-2">
                <Label htmlFor="sound-file">
                  {t('settings.general.notifications.sound.fileLabel')}
                </Label>
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
                <p className="text-sm text-muted-foreground">
                  {t('settings.general.notifications.sound.fileHelper')}
                </p>
              </div>
              <div className="ml-6 space-y-2">
                <Label htmlFor="error-sound-file">Error Sound</Label>
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
                <p className="text-sm text-muted-foreground">
                  Sound played when a task fails or an error occurs
                </p>
              </div>
            </>
          )}
          <div className="flex items-center space-x-2">
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
            <div className="space-y-0.5">
              <Label htmlFor="push-notifications" className="cursor-pointer">
                {t('settings.general.notifications.push.label')}
              </Label>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.notifications.push.helper')}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.concurrency.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.concurrency.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="max-concurrent-agents">
              {t('settings.general.concurrency.maxAgents.label')}
            </Label>
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
            />
            <p className="text-sm text-muted-foreground">
              {t('settings.general.concurrency.maxAgents.helper')}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.backup.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.backup.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center space-x-2">
            <Checkbox
              id="backup-enabled"
              checked={draft?.backup.enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({
                  backup: { ...draft!.backup, enabled: checked },
                })
              }
            />
            <div className="space-y-0.5">
              <Label htmlFor="backup-enabled" className="cursor-pointer">
                {t('settings.general.backup.enabled.label')}
              </Label>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.backup.enabled.helper')}
              </p>
            </div>
          </div>

          {draft?.backup.enabled && (
            <>
              <div className="ml-6 space-y-2">
                <Label htmlFor="backup-interval">
                  {t('settings.general.backup.interval.label')}
                </Label>
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
                <p className="text-sm text-muted-foreground">
                  {t('settings.general.backup.interval.helper')}
                </p>
              </div>

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
                    <p className="text-sm text-muted-foreground">
                      {t('settings.general.backup.advanced.description')}
                    </p>

                    <div className="space-y-2">
                      <Label htmlFor="backup-hours-all">
                        {t('settings.general.backup.advanced.hoursAll.label')}
                      </Label>
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
                      <p className="text-sm text-muted-foreground">
                        {t('settings.general.backup.advanced.hoursAll.helper')}
                      </p>
                    </div>

                    <div className="space-y-2">
                      <Label htmlFor="backup-daily-days">
                        {t('settings.general.backup.advanced.dailyDays.label')}
                      </Label>
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
                      <p className="text-sm text-muted-foreground">
                        {t('settings.general.backup.advanced.dailyDays.helper')}
                      </p>
                    </div>

                    <div className="space-y-2">
                      <Label htmlFor="backup-weekly-weeks">
                        {t('settings.general.backup.advanced.weeklyWeeks.label')}
                      </Label>
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
                      <p className="text-sm text-muted-foreground">
                        {t('settings.general.backup.advanced.weeklyWeeks.helper')}
                      </p>
                    </div>

                    <div className="space-y-2">
                      <Label htmlFor="backup-monthly-months">
                        {t('settings.general.backup.advanced.monthlyMonths.label')}
                      </Label>
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
                      <p className="text-sm text-muted-foreground">
                        {t('settings.general.backup.advanced.monthlyMonths.helper')}
                      </p>
                    </div>
                  </div>
                )}
              </div>
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.privacy.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.privacy.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center space-x-2">
            <Checkbox
              id="analytics-enabled"
              checked={draft?.analytics_enabled ?? false}
              onCheckedChange={(checked: boolean) =>
                updateDraft({ analytics_enabled: checked })
              }
            />
            <div className="space-y-0.5">
              <Label htmlFor="analytics-enabled" className="cursor-pointer">
                {t('settings.general.privacy.telemetry.label')}
              </Label>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.privacy.telemetry.helper')}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.taskTemplates.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.taskTemplates.description')}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <TagManager />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.general.safety.title')}</CardTitle>
          <CardDescription>
            {t('settings.general.safety.description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <p className="font-medium">
                {t('settings.general.safety.disclaimer.title')}
              </p>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.safety.disclaimer.description')}
              </p>
            </div>
            <Button variant="outline" onClick={resetDisclaimer}>
              {t('settings.general.safety.disclaimer.button')}
            </Button>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="font-medium">
                {t('settings.general.safety.onboarding.title')}
              </p>
              <p className="text-sm text-muted-foreground">
                {t('settings.general.safety.onboarding.description')}
              </p>
            </div>
            <Button variant="outline" onClick={resetOnboarding}>
              {t('settings.general.safety.onboarding.button')}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Sticky Save Button */}
      <div className="sticky bottom-0 z-10 bg-background/80 backdrop-blur-sm border-t py-4">
        <div className="flex items-center justify-between">
          {hasUnsavedChanges ? (
            <span className="text-sm text-muted-foreground">
              {t('settings.general.save.unsavedChanges')}
            </span>
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
