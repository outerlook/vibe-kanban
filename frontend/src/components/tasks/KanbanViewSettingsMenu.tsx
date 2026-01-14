import { useTranslation } from 'react-i18next';
import { SlidersHorizontal } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useKanbanViewStore } from '@/stores/useKanbanViewStore';

export function KanbanViewSettingsMenu() {
  const { t } = useTranslation('tasks');
  const { isCompact, toggleCompact } = useKanbanViewStore();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="h-9 px-2 text-muted-foreground hover:text-foreground"
          title={t('taskFilterBar.viewSettings', 'View settings')}
        >
          <SlidersHorizontal className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuCheckboxItem
          checked={isCompact}
          onCheckedChange={toggleCompact}
        >
          {t('taskFilterBar.compactView', 'Compact view')}
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
