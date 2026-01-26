import { useState, useEffect } from 'react';
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
import { Textarea } from '@/components/ui/textarea';
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
}

const MAIN_REPO_VALUE = '__main__';

const NewConversationDialogImpl = NiceModal.create<NewConversationDialogProps>(
  (props) => {
    const modal = useModal();
    const { t } = useTranslation(['common']);
    const { projectId, defaultWorktreePath } = props;
    const { profiles, config } = useUserSystem();

    const [title, setTitle] = useState('');
    const [initialMessage, setInitialMessage] = useState('');
    const [error, setError] = useState<string | null>(null);
    const [selectedProfile, setSelectedProfile] =
      useState<ExecutorProfileId | null>(null);
    const [selectedWorktree, setSelectedWorktree] = useState<string>(
      defaultWorktreePath ?? MAIN_REPO_VALUE
    );

    const { data: worktreesData } = useWorktrees(projectId);
    const worktrees = worktreesData?.worktrees ?? [];
    const nonMainWorktrees = worktrees.filter((w) => !w.is_main);
    const hasWorktrees = nonMainWorktrees.length > 0;

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
        setSelectedWorktree(defaultWorktreePath ?? MAIN_REPO_VALUE);
      }
    }, [modal.visible, defaultWorktreePath]);

    const canSubmit = !!title.trim() && !!initialMessage.trim() && !isLoading;

    const handleSubmit = async () => {
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
        const selectedWorktreeInfo =
          selectedWorktree !== MAIN_REPO_VALUE
            ? worktrees.find((w) => w.path === selectedWorktree)
            : null;

        const result = await createMutation.mutateAsync({
          projectId,
          data: {
            title: trimmedTitle,
            initial_message: trimmedMessage,
            executor_profile_id: effectiveProfile,
            worktree_path: selectedWorktreeInfo?.path ?? null,
            worktree_branch: selectedWorktreeInfo?.branch ?? null,
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
    };

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
              <Textarea
                id="initial-message"
                value={initialMessage}
                onChange={(e) => setInitialMessage(e.target.value)}
                placeholder={t('conversations.initialMessagePlaceholder', {
                  defaultValue: 'What would you like help with?',
                })}
                rows={4}
                className="resize-none"
              />
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
