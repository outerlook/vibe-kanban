import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, X } from 'lucide-react';
import { useTaskFilters } from '@/hooks/useTaskFilters';
import { useTaskGroupsContext } from '@/contexts/TaskGroupsContext';
import { useSearch } from '@/contexts/SearchContext';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { KanbanViewSettingsMenu } from './KanbanViewSettingsMenu';

const ALL_GROUPS_VALUE = '__all__';

export function TaskFilterBar() {
  const { t } = useTranslation('tasks');
  const {
    filters,
    setSearch,
    setGroupId,
    setHideBlocked,
    clearFilters,
    hasActiveFilters,
  } = useTaskFilters();
  const { groups } = useTaskGroupsContext();
  const { registerInputRef } = useSearch();

  const inputRef = useRef<HTMLInputElement>(null);
  const [localSearch, setLocalSearch] = useState(filters.search);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Sync local state when URL changes externally
  useEffect(() => {
    setLocalSearch(filters.search);
  }, [filters.search]);

  // Debounced search update
  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    debounceRef.current = setTimeout(() => {
      if (localSearch !== filters.search) {
        setSearch(localSearch);
      }
    }, 300);

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, [localSearch, filters.search, setSearch]);

  // Register input ref for keyboard shortcut focus (via useKeyFocusSearch in ProjectTasks)
  useEffect(() => {
    registerInputRef(inputRef.current);
    return () => registerInputRef(null);
  }, [registerInputRef]);

  const handleSearchChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      setLocalSearch(e.target.value);
    },
    []
  );

  const handleGroupChange = useCallback(
    (value: string) => {
      setGroupId(value === ALL_GROUPS_VALUE ? null : value);
    },
    [setGroupId]
  );

  const handleHideBlockedChange = useCallback(
    (checked: boolean) => {
      setHideBlocked(checked);
    },
    [setHideBlocked]
  );

  return (
    <div className="flex flex-wrap items-center gap-3 py-2">
      {/* Group Dropdown */}
      <Select
        value={filters.groupId ?? ALL_GROUPS_VALUE}
        onValueChange={handleGroupChange}
      >
        <SelectTrigger className="w-[180px] h-9 rounded-md bg-background">
          <SelectValue placeholder={t('taskFormDialog.groupPlaceholder', 'Select group...')} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={ALL_GROUPS_VALUE}>
            {t('taskFilterBar.allGroups', 'All Groups')}
          </SelectItem>
          {groups.map((group) => (
            <SelectItem key={group.id} value={group.id}>
              {group.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* Search Input */}
      <div className="relative flex-1 min-w-[200px] max-w-[300px]">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground pointer-events-none" />
        <Input
          ref={inputRef}
          type="text"
          value={localSearch}
          onChange={handleSearchChange}
          placeholder={t('taskFilterBar.searchPlaceholder', 'Search tasks...')}
          className="pl-9 h-9 rounded-md"
        />
      </div>

      {/* Hide Blocked Toggle */}
      <label className="flex items-center gap-2 text-sm text-muted-foreground cursor-pointer">
        <Checkbox
          checked={filters.hideBlocked}
          onCheckedChange={handleHideBlockedChange}
        />
        {t('taskFilterBar.hideBlocked', 'Hide blocked')}
      </label>

      {/* Clear Filters Button */}
      {hasActiveFilters && (
        <Button
          variant="ghost"
          size="sm"
          onClick={clearFilters}
          className="h-9 px-2 text-muted-foreground hover:text-foreground"
          title={t('taskFilterBar.clearFilters', 'Clear filters')}
        >
          <X className="h-4 w-4" />
        </Button>
      )}

      {/* View Settings */}
      <KanbanViewSettingsMenu />
    </div>
  );
}
