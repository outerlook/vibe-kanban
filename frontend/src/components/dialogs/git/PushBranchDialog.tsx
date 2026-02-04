import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { GitBranch, Loader2, Upload } from 'lucide-react';
import { defineModal } from '@/lib/modals';
import { usePushBranch } from '@/hooks/usePushBranch';
import { useState } from 'react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import type { PushBranchError } from 'shared/types';

export interface PushBranchDialogProps {
  repoId: string;
  branchName: string;
  commitsAhead?: number;
}

export type PushBranchDialogResult = 'success' | 'canceled' | 'force_push_required';

const PushBranchDialogImpl = NiceModal.create<PushBranchDialogProps>((props) => {
  const modal = useModal();
  const { repoId, branchName, commitsAhead } = props;
  const [error, setError] = useState<string | null>(null);

  const pushBranch = usePushBranch(
    () => {
      modal.resolve('success' as PushBranchDialogResult);
      modal.hide();
    },
    (_err: unknown, errorData?: PushBranchError) => {
      if (errorData?.type === 'force_push_required') {
        modal.resolve('force_push_required' as PushBranchDialogResult);
        modal.hide();
        return;
      }

      const errorMessage = getErrorMessage(errorData);
      setError(errorMessage);
    }
  );

  const handleConfirm = async () => {
    setError(null);
    try {
      await pushBranch.mutateAsync({ repoId, branchName, force: false });
    } catch {
      // Error already handled by onError callback
    }
  };

  const handleCancel = () => {
    modal.resolve('canceled' as PushBranchDialogResult);
    modal.hide();
  };

  const isProcessing = pushBranch.isPending;

  const commitsMessage =
    commitsAhead !== undefined && commitsAhead > 0
      ? `${commitsAhead} commit${commitsAhead === 1 ? '' : 's'} ahead of remote`
      : null;

  return (
    <Dialog open={modal.visible} onOpenChange={handleCancel}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <div className="flex items-center gap-3">
            <Upload className="h-6 w-6 text-primary" />
            <DialogTitle>Push to Remote</DialogTitle>
          </div>
          <DialogDescription className="text-left pt-2 space-y-2">
            <div className="flex items-center gap-2">
              <GitBranch className="h-4 w-4 text-muted-foreground" />
              <span className="font-medium">{branchName}</span>
            </div>
            {commitsMessage && (
              <p className="text-sm text-muted-foreground">{commitsMessage}</p>
            )}
          </DialogDescription>
        </DialogHeader>
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
        <DialogFooter className="gap-2">
          <Button variant="outline" onClick={handleCancel} disabled={isProcessing}>
            Cancel
          </Button>
          <Button onClick={handleConfirm} disabled={isProcessing}>
            {isProcessing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {isProcessing ? 'Pushing...' : 'Push'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
});

function getErrorMessage(errorData?: PushBranchError): string {
  if (!errorData) return 'Push failed';

  switch (errorData.type) {
    case 'no_remote_tracking':
      return 'Branch has no remote tracking configured';
    case 'auth_failed':
      return 'Authentication failed. Please check your credentials.';
    default:
      return 'Push failed';
  }
}

export const PushBranchDialog = defineModal<PushBranchDialogProps, PushBranchDialogResult>(
  PushBranchDialogImpl
);
