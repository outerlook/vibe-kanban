import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import { Button } from '@/components/ui/button';
import { useState, useEffect, useCallback } from 'react';
import { Loader2, RefreshCw } from 'lucide-react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import { useTranslation } from 'react-i18next';

interface CommitMessagePreviewDialogProps {
  initialMessage: string;
  onConfirm: (message: string) => Promise<void>;
  onRegenerate?: () => Promise<string>;
}

type DialogResult = 'confirmed' | 'canceled';

const CommitMessagePreviewDialogImpl =
  NiceModal.create<CommitMessagePreviewDialogProps>(
    ({ initialMessage, onConfirm, onRegenerate }) => {
      const modal = useModal();
      const { t } = useTranslation('tasks');
      const [message, setMessage] = useState(initialMessage);
      const [isMerging, setIsMerging] = useState(false);
      const [isRegenerating, setIsRegenerating] = useState(false);

      useEffect(() => {
        if (modal.visible) {
          setMessage(initialMessage);
        }
      }, [modal.visible, initialMessage]);

      const handleRegenerate = useCallback(async () => {
        if (!onRegenerate) return;
        setIsRegenerating(true);
        try {
          const newMessage = await onRegenerate();
          setMessage(newMessage);
        } finally {
          setIsRegenerating(false);
        }
      }, [onRegenerate]);

      const handleMerge = useCallback(async () => {
        setIsMerging(true);
        try {
          await onConfirm(message);
          modal.resolve('confirmed' as DialogResult);
          modal.hide();
        } finally {
          setIsMerging(false);
        }
      }, [message, onConfirm, modal]);

      const handleCancel = useCallback(() => {
        modal.resolve('canceled' as DialogResult);
        modal.hide();
      }, [modal]);

      const isLoading = isMerging || isRegenerating;

      return (
        <Dialog open={modal.visible} onOpenChange={() => handleCancel()}>
          <DialogContent className="sm:max-w-[525px]">
            <DialogHeader>
              <DialogTitle>
                {t('commitMessageDialog.title', 'Commit Message')}
              </DialogTitle>
            </DialogHeader>
            <div className="py-4">
              <Textarea
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder={t(
                  'commitMessageDialog.placeholder',
                  'Enter commit message...'
                )}
                rows={6}
                disabled={isLoading}
                className={isLoading ? 'opacity-50 cursor-not-allowed' : ''}
              />
            </div>
            <DialogFooter className={onRegenerate ? 'sm:justify-between' : 'sm:justify-end'}>
              {onRegenerate && (
                <Button
                  variant="outline"
                  onClick={handleRegenerate}
                  disabled={isLoading}
                >
                  {isRegenerating ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      {t('commitMessageDialog.regenerating', 'Regenerating...')}
                    </>
                  ) : (
                    <>
                      <RefreshCw className="mr-2 h-4 w-4" />
                      {t('commitMessageDialog.regenerate', 'Regenerate')}
                    </>
                  )}
                </Button>
              )}
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  onClick={handleCancel}
                  disabled={isLoading}
                >
                  {t('common:buttons.cancel', 'Cancel')}
                </Button>
                <Button
                  onClick={handleMerge}
                  disabled={isLoading || !message.trim()}
                  className="bg-blue-600 hover:bg-blue-700"
                >
                  {isMerging ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      {t('commitMessageDialog.merging', 'Merging...')}
                    </>
                  ) : (
                    t('commitMessageDialog.merge', 'Merge')
                  )}
                </Button>
              </div>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      );
    }
  );

export const CommitMessagePreviewDialog = defineModal<
  CommitMessagePreviewDialogProps,
  DialogResult
>(CommitMessagePreviewDialogImpl);
