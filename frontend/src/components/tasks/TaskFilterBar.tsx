import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Circle,
  Clock,
  CheckCircle,
  XCircle,
  Search,
  X,
  Eye,
} from 'lucide-react';
import type { TaskStatus } from 'shared/types';
import { TASK_STATUSES } from '@/constants/taskStatuses';
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
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { statusLabels } from '@/utils/statusLabels';

const ALL_GROUPS_VALUE = '__all__';

const STATUS_ICONS: Record<TaskStatus, React.ReactNode> = {
  todo: <Circle className="h-3.5 w-3.5" />,
  inprogress: <Clock className="h-3.5 w-3.5" />,
  inreview: <Eye className="h-3.5 w-3.5" />,
  done: <CheckCircle className="h-3.5 w-3.5" />,
  cancelled: <XCircle className="h-3.5 w-3.5" />,
};

export function TaskFilterBar() {
  const { t } = useTranslation('tasks');
  const { filters, setSearch, setGroupId, setStatuses, clearFilters, hasActiveFilters } =
    useTaskFilters();
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

  const handleStatusToggle = useCallback(
    (values: string[]) => {
      setStatuses(values as TaskStatus[]);
    },
    [setStatuses]
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

      {/* Status Toggles */}
      <ToggleGroup
        type="multiple"
        value={filters.statuses}
        onValueChange={handleStatusToggle}
        className="flex gap-1"
      >
        {TASK_STATUSES.map((status) => {
          const isActive = filters.statuses.includes(status);
          return (
            <ToggleGroupItem
              key={status}
              value={status}
              active={isActive}
              title={statusLabels[status]}
              className={cn(
                'h-9 w-9 p-0 rounded-md border',
                isActive
                  ? 'bg-primary text-primary-foreground border-primary'
                  : 'bg-background text-muted-foreground border-input hover:bg-accent hover:text-accent-foreground'
              )}
            >
              {STATUS_ICONS[status]}
            </ToggleGroupItem>
          );
        })}
      </ToggleGroup>

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
    </div>
  );
}
