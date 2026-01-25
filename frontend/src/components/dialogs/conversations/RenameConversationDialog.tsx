import { useEffect, useState } from 'react';
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
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal, getErrorMessage } from '@/lib/modals';
import { useUpdateConversation } from '@/hooks/useConversations';

export interface RenameConversationDialogProps {
  conversationId: string;
  currentTitle: string;
}

export type RenameConversationDialogResult = {
  action: 'confirmed' | 'canceled';
  title?: string;
};

const RenameConversationDialogImpl =
  NiceModal.create<RenameConversationDialogProps>(
    ({ conversationId, currentTitle }) => {
      const modal = useModal();
      const [title, setTitle] = useState<string>(currentTitle);
      const [error, setError] = useState<string | null>(null);

      useEffect(() => {
        setTitle(currentTitle);
        setError(null);
      }, [currentTitle]);

      const updateMutation = useUpdateConversation();

      const handleConfirm = async () => {
        const trimmedTitle = title.trim();

        if (!trimmedTitle) {
          setError('Title cannot be empty');
          return;
        }

        if (trimmedTitle === currentTitle) {
          modal.resolve({ action: 'canceled' } as RenameConversationDialogResult);
          modal.hide();
          return;
        }

        setError(null);

        try {
          await updateMutation.mutateAsync({
            conversationId,
            data: { title: trimmedTitle },
          });
          modal.resolve({
            action: 'confirmed',
            title: trimmedTitle,
          } as RenameConversationDialogResult);
          modal.hide();
        } catch (err: unknown) {
          setError(getErrorMessage(err) || 'Failed to rename conversation');
        }
      };

      const handleCancel = () => {
        modal.resolve({ action: 'canceled' } as RenameConversationDialogResult);
        modal.hide();
      };

      const handleOpenChange = (open: boolean) => {
        if (!open) {
          handleCancel();
        }
      };

      return (
        <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle>Rename Conversation</DialogTitle>
              <DialogDescription>
                Enter a new title for this conversation.
              </DialogDescription>
            </DialogHeader>

            <div className="space-y-4">
              <div className="space-y-2">
                <label htmlFor="conversation-title" className="text-sm font-medium">
                  Title
                </label>
                <Input
                  id="conversation-title"
                  type="text"
                  value={title}
                  onChange={(e) => {
                    setTitle(e.target.value);
                    setError(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && !updateMutation.isPending) {
                      handleConfirm();
                    }
                  }}
                  placeholder="Enter conversation title"
                  disabled={updateMutation.isPending}
                  autoFocus
                />
                {error && <p className="text-sm text-destructive">{error}</p>}
              </div>
            </div>

            <DialogFooter>
              <Button
                variant="outline"
                onClick={handleCancel}
                disabled={updateMutation.isPending}
              >
                Cancel
              </Button>
              <Button
                onClick={handleConfirm}
                disabled={updateMutation.isPending || !title.trim()}
              >
                {updateMutation.isPending ? 'Saving...' : 'Save'}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      );
    }
  );

export const RenameConversationDialog = defineModal<
  RenameConversationDialogProps,
  RenameConversationDialogResult
>(RenameConversationDialogImpl);
