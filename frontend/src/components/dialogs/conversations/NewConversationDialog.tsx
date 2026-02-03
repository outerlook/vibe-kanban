import { useState, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import WYSIWYGEditor from '@/components/ui/wysiwyg';
import { imagesApi } from '@/lib/api';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ExecutorProfileSelector } from '@/components/settings';
import { useCreateConversation } from '@/hooks/useConversations';
import { useWorktrees } from '@/hooks/useWorktrees';
import { useUserSystem } from '@/components/ConfigProvider';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import type { ConversationSession, ExecutorProfileId } from 'shared/types';
import { Home, GitBranch } from 'lucide-react';

export interface NewConversationDialogProps {
  projectId: string;
  defaultWorktreePath?: string;
  defaultBaseBranch?: string;
}

const MAIN_REPO_VALUE = '__main__';
const BRANCH_ONLY_PREFIX = '__branch__:';

const NewConversationDialogImpl = NiceModal.create<NewConversationDialogProps>(
  (props) => {
    const modal = useModal();
    const { t } = useTranslation(['common']);
    const { projectId, defaultWorktreePath, defaultBaseBranch } = props;
    const { profiles, config } = useUserSystem();

    const [title, setTitle] = useState('');
    const [initialMessage, setInitialMessage] = useState('');
    const [error, setError] = useState<string | null>(null);
    const [selectedProfile, setSelectedProfile] =
      useState<ExecutorProfileId | null>(null);

    // Determine initial selection: worktree path, branch-only, or main
    const getInitialSelection = () => {
      if (defaultWorktreePath) return defaultWorktreePath;
      if (defaultBaseBranch) return `${BRANCH_ONLY_PREFIX}${defaultBaseBranch}`;
      return MAIN_REPO_VALUE;
    };
    const [selectedWorktree, setSelectedWorktree] = useState<string>(
      getInitialSelection()
    );

    const { data: worktreesData } = useWorktrees(projectId);
    const worktrees = useMemo(
      () => worktreesData?.worktrees ?? [],
      [worktreesData?.worktrees]
    );
    const nonMainWorktrees = useMemo(
      () => worktrees.filter((w) => !w.is_main),
      [worktrees]
    );
    // Show selector if there are worktrees OR if a defaultBaseBranch is provided
    const hasWorktrees = nonMainWorktrees.length > 0 || !!defaultBaseBranch;

    const createMutation = useCreateConversation();
    const isLoading = createMutation.isPending;

    const defaultProfile = config?.executor_profile ?? null;
    const effectiveProfile = selectedProfile ?? defaultProfile;

    // Reset form when dialog opens
    useEffect(() => {
      if (modal.visible) {
        setTitle('');
        setInitialMessage('');
        setError(null);
        setSelectedProfile(null);
        if (defaultWorktreePath) {
          setSelectedWorktree(defaultWorktreePath);
        } else if (defaultBaseBranch) {
          setSelectedWorktree(`${BRANCH_ONLY_PREFIX}${defaultBaseBranch}`);
        } else {
          setSelectedWorktree(MAIN_REPO_VALUE);
        }
      }
    }, [modal.visible, defaultWorktreePath, defaultBaseBranch]);

    const canSubmit = !!title.trim() && !!initialMessage.trim() && !isLoading;

    const handleSubmit = useCallback(async () => {
      const trimmedTitle = title.trim();
      const trimmedMessage = initialMessage.trim();

      if (!trimmedTitle) {
        setError(
          t('conversations.errors.titleRequired', {
            defaultValue: 'Title is required',
          })
        );
        return;
      }

      if (!trimmedMessage) {
        setError(
          t('conversations.errors.messageRequired', {
            defaultValue: 'Initial message is required',
          })
        );
        return;
      }

      setError(null);

      try {
        // Determine worktree path and branch based on selection
        let worktreePath: string | null = null;
        let worktreeBranch: string | null = null;

        if (selectedWorktree.startsWith(BRANCH_ONLY_PREFIX)) {
          // Branch-only mode: no worktree path, just the branch name
          worktreeBranch = selectedWorktree.slice(BRANCH_ONLY_PREFIX.length);
        } else if (selectedWorktree !== MAIN_REPO_VALUE) {
          // Regular worktree selection
          const selectedWorktreeInfo = worktrees.find(
            (w) => w.path === selectedWorktree
          );
          worktreePath = selectedWorktreeInfo?.path ?? null;
          worktreeBranch = selectedWorktreeInfo?.branch ?? null;
        }

        const result = await createMutation.mutateAsync({
          projectId,
          data: {
            title: trimmedTitle,
            initial_message: trimmedMessage,
            executor_profile_id: effectiveProfile,
            worktree_path: worktreePath,
            worktree_branch: worktreeBranch,
          },
        });

        modal.resolve(result.session);
        modal.hide();
      } catch {
        setError(
          t('conversations.errors.createFailed', {
            defaultValue: 'Failed to create conversation',
          })
        );
      }
    }, [
      title,
      initialMessage,
      t,
      selectedWorktree,
      worktrees,
      createMutation,
      projectId,
      effectiveProfile,
      modal,
    ]);

    const handleOpenChange = (open: boolean) => {
      if (!open) {
        modal.resolve(null);
        modal.hide();
      }
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && canSubmit) {
        e.preventDefault();
        handleSubmit();
      }
    };

    // Handle Cmd+Enter from the WYSIWYG editor
    const handleEditorCmdEnter = useCallback(() => {
      if (canSubmit) {
        handleSubmit();
      }
    }, [canSubmit, handleSubmit]);

    // Handle image paste - upload globally (no conversation ID yet)
    const handlePasteFiles = useCallback(async (files: File[]) => {
      for (const file of files) {
        try {
          const response = await imagesApi.upload(file);
          const imageMarkdown = `![${response.original_name}](${response.file_path})`;
          setInitialMessage((prev) =>
            prev ? `${prev}\n\n${imageMarkdown}` : imageMarkdown
          );
        } catch (error) {
          console.error('Failed to upload image:', error);
        }
      }
    }, []);

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-[500px]" onKeyDown={handleKeyDown}>
          <DialogHeader>
            <DialogTitle>
              {t('conversations.newTitle', {
                defaultValue: 'New Conversation',
              })}
            </DialogTitle>
            <DialogDescription>
              {t('conversations.newDescription', {
                defaultValue:
                  'Start a new conversation with the AI assistant.',
              })}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="conversation-title">
                {t('conversations.titleLabel', { defaultValue: 'Title' })}
              </Label>
              <Input
                id="conversation-title"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder={t('conversations.titlePlaceholder', {
                  defaultValue: 'e.g., Help with feature implementation',
                })}
                autoFocus
              />
            </div>

            {profiles && (
              <div className="space-y-2">
                <ExecutorProfileSelector
                  profiles={profiles}
                  selectedProfile={effectiveProfile}
                  onProfileSelect={setSelectedProfile}
                  showLabel={true}
                />
              </div>
            )}

            {hasWorktrees && (
              <div className="space-y-2">
                <Label htmlFor="worktree-select">
                  {t('conversations.worktreeLabel', {
                    defaultValue: 'Start from',
                  })}
                </Label>
                <Select
                  value={selectedWorktree}
                  onValueChange={setSelectedWorktree}
                >
                  <SelectTrigger id="worktree-select">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={MAIN_REPO_VALUE}>
                      <div className="flex items-center gap-2">
                        <Home className="h-4 w-4" />
                        <span>
                          {t('conversations.mainRepository', {
                            defaultValue: 'Main repository',
                          })}
                        </span>
                      </div>
                    </SelectItem>
                    {defaultBaseBranch && (
                      <SelectItem
                        value={`${BRANCH_ONLY_PREFIX}${defaultBaseBranch}`}
                      >
                        <div className="flex items-center gap-2">
                          <GitBranch className="h-4 w-4" />
                          <span>{defaultBaseBranch}</span>
                          <span className="text-muted-foreground text-xs ml-1">
                            {t('conversations.branchOnly', {
                              defaultValue: '(branch)',
                            })}
                          </span>
                        </div>
                      </SelectItem>
                    )}
                    {nonMainWorktrees.map((worktree) => (
                      <SelectItem key={worktree.path} value={worktree.path}>
                        <div className="flex items-center gap-2">
                          <GitBranch className="h-4 w-4" />
                          <span>{worktree.branch ?? worktree.path}</span>
                          {worktree.branch && (
                            <span className="text-muted-foreground text-xs ml-1 truncate max-w-[200px]">
                              {worktree.path}
                            </span>
                          )}
                        </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}

            <div className="space-y-2">
              <Label htmlFor="initial-message">
                {t('conversations.initialMessageLabel', {
                  defaultValue: 'Initial Message',
                })}
              </Label>
              <div className="border rounded-md p-2">
                <WYSIWYGEditor
                  value={initialMessage}
                  onChange={setInitialMessage}
                  placeholder={t('conversations.initialMessagePlaceholder', {
                    defaultValue: 'What would you like help with?',
                  })}
                  onPasteFiles={handlePasteFiles}
                  onCmdEnter={handleEditorCmdEnter}
                  projectId={projectId}
                  className="min-h-[100px]"
                  autoFocus={false}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                {t('conversations.initialMessageHint', {
                  defaultValue: 'Press Cmd+Enter to create',
                })}
              </p>
            </div>

            {error && <div className="text-sm text-destructive">{error}</div>}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={isLoading}
            >
              {t('common:buttons.cancel', { defaultValue: 'Cancel' })}
            </Button>
            <Button onClick={handleSubmit} disabled={!canSubmit}>
              {isLoading
                ? t('conversations.creating', { defaultValue: 'Creating...' })
                : t('conversations.create', { defaultValue: 'Create' })}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const NewConversationDialog = defineModal<
  NewConversationDialogProps,
  ConversationSession | null
>(NewConversationDialogImpl);
