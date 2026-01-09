import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { useTaskSelection } from '@/contexts/TaskSelectionContext';
import { X } from 'lucide-react';

export function BulkActionsBar() {
  const { t } = useTranslation('tasks');
  const { selectedCount, clearSelection, getSelectedIds } = useTaskSelection();

  if (selectedCount === 0) {
    return null;
  }

  const handleCreateAttempts = () => {
    const ids = getSelectedIds();
    console.log('Create attempts for tasks:', ids);
    // TODO: Open BulkCreateAttemptsDialog when implemented
  };

  return createPortal(
    <Card className="fixed bottom-8 left-1/2 -translate-x-1/2 z-40 flex items-center gap-4 px-4 py-3 shadow-lg border rounded-lg">
      <span className="text-sm font-medium">
        {t('bulkActions.selectedCount', { count: selectedCount })}
      </span>
      <div className="flex items-center gap-2">
        <Button variant="default" size="sm" onClick={handleCreateAttempts}>
          {t('bulkActions.createAttempts')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={clearSelection}
          className="text-muted-foreground"
        >
          <X className="h-4 w-4 mr-1" />
          {t('bulkActions.clearSelection')}
        </Button>
      </div>
    </Card>,
    document.body
  );
}
