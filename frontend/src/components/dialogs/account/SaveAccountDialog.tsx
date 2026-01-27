import { useState } from 'react';
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
import { useSaveClaudeAccount } from '@/hooks/useClaudeAccounts';
import type { SavedAccount } from 'shared/types';

export interface SaveAccountDialogProps {
  currentSubscriptionType?: string;
}

const SaveAccountDialogImpl = NiceModal.create<SaveAccountDialogProps>(
  ({ currentSubscriptionType }) => {
    const modal = useModal();
    const [name, setName] = useState('');
    const [error, setError] = useState<string | null>(null);

    const saveMutation = useSaveClaudeAccount();

    const handleSave = async () => {
      setError(null);

      try {
        const trimmedName = name.trim();
        const account = await saveMutation.mutateAsync(
          trimmedName || undefined
        );
        modal.resolve(account);
        modal.hide();
      } catch (err: unknown) {
        setError(getErrorMessage(err) || 'Failed to save account');
      }
    };

    const handleCancel = () => {
      modal.resolve(null);
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
            <DialogTitle>Save Current Account</DialogTitle>
            <DialogDescription>
              {currentSubscriptionType
                ? `Save your ${currentSubscriptionType} account for quick switching.`
                : 'Save your current account for quick switching.'}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <label htmlFor="account-name" className="text-sm font-medium">
                Account Name (optional)
              </label>
              <Input
                id="account-name"
                type="text"
                value={name}
                onChange={(e) => {
                  setName(e.target.value);
                  setError(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !saveMutation.isPending) {
                    handleSave();
                  }
                }}
                placeholder="e.g., Work, Personal"
                disabled={saveMutation.isPending}
                autoFocus
              />
              <p className="text-xs text-muted-foreground">
                Leave empty to use hash identifier
              </p>
              {error && <p className="text-sm text-destructive">{error}</p>}
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={saveMutation.isPending}
            >
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={saveMutation.isPending}>
              {saveMutation.isPending ? 'Saving...' : 'Save'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const SaveAccountDialog = defineModal<
  SaveAccountDialogProps,
  SavedAccount | null
>(SaveAccountDialogImpl);
