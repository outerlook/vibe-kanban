import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { AlertTriangle, GitBranch, Loader2 } from 'lucide-react';
import { defineModal } from '@/lib/modals';
import { usePushBranch } from '@/hooks/usePushBranch';
import { useState } from 'react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import type { PushBranchError } from 'shared/types';

export interface ForcePushBranchDialogProps {
  repoId: string;
  branchName: string;
}

export type ForcePushBranchDialogResult = 'success' | 'canceled';

const ForcePushBranchDialogImpl = NiceModal.create<ForcePushBranchDialogProps>((props) => {
  const modal = useModal();
  const { repoId, branchName } = props;
  const [error, setError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);

  const pushBranch = usePushBranch(
    () => {
      modal.resolve('success' as ForcePushBranchDialogResult);
      modal.hide();
    },
    (_err: unknown, errorData?: PushBranchError) => {
      const errorMessage = getErrorMessage(errorData);
      setError(errorMessage);
    }
  );

  const handleConfirm = async () => {
    setError(null);
    try {
      await pushBranch.mutateAsync({ repoId, branchName, force: true });
    } catch {
      // Error already handled by onError callback
    }
  };

  const handleCancel = () => {
    modal.resolve('canceled' as ForcePushBranchDialogResult);
    modal.hide();
  };

  const isProcessing = pushBranch.isPending;

  return (
    <Dialog open={modal.visible} onOpenChange={handleCancel}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <div className="flex items-center gap-3">
            <AlertTriangle className="h-6 w-6 text-destructive" />
            <DialogTitle>Force Push Required</DialogTitle>
          </div>
          <DialogDescription className="text-left pt-2 space-y-3">
            <div className="flex items-center gap-2">
              <GitBranch className="h-4 w-4 text-muted-foreground" />
              <span className="font-medium">{branchName}</span>
            </div>
            <p>
              The remote branch has changes that your local branch doesn't have.
              Force pushing will overwrite these remote changes.
            </p>
            <p className="font-medium text-destructive">
              This action cannot be undone and may cause data loss.
            </p>
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-start space-x-3 py-2">
          <Checkbox
            id="confirm-force-push"
            checked={confirmed}
            onCheckedChange={(checked) => setConfirmed(checked === true)}
            disabled={isProcessing}
          />
          <label
            htmlFor="confirm-force-push"
            className="text-sm leading-tight cursor-pointer select-none"
          >
            I understand this may overwrite remote changes
          </label>
        </div>

        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
        <DialogFooter className="gap-2">
          <Button variant="outline" onClick={handleCancel} disabled={isProcessing}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={handleConfirm}
            disabled={isProcessing || !confirmed}
          >
            {isProcessing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {isProcessing ? 'Force Pushing...' : 'Force Push'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
});

function getErrorMessage(errorData?: PushBranchError): string {
  if (!errorData) return 'Force push failed';

  switch (errorData.type) {
    case 'no_remote_tracking':
      return 'Branch has no remote tracking configured';
    case 'auth_failed':
      return 'Authentication failed. Please check your credentials.';
    case 'force_push_required':
      return 'Push rejected. Please try again.';
    default:
      return 'Force push failed';
  }
}

export const ForcePushBranchDialog = defineModal<ForcePushBranchDialogProps, ForcePushBranchDialogResult>(
  ForcePushBranchDialogImpl
);
