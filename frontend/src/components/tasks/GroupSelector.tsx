import { useState, useMemo, useRef, useEffect, useCallback, memo } from 'react';
import { Virtuoso, VirtuosoHandle } from 'react-virtuoso';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button.tsx';
import { ArrowDown, Layers, Search, GitBranch } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu.tsx';
import { Input } from '@/components/ui/input.tsx';
import type { TaskGroup } from 'shared/types';

type GroupSelectorProps = {
  groups: TaskGroup[];
  selectedGroupId: string | null;
  onGroupSelect: (groupId: string | null) => void;
  placeholder?: string;
  allowNone?: boolean;
  allowAll?: boolean;
  className?: string;
  disabled?: boolean;
};

type GroupRowProps = {
  group: TaskGroup;
  isSelected: boolean;
  isHighlighted: boolean;
  onHover: () => void;
  onSelect: () => void;
};

const GroupRow = memo(function GroupRow({
  group,
  isSelected,
  isHighlighted,
  onHover,
  onSelect,
}: GroupRowProps) {
  const classes =
    (isSelected ? 'bg-accent text-accent-foreground ' : '') +
    (!isSelected && isHighlighted ? 'bg-accent/70 ring-2 ring-accent ' : '') +
    'transition-none';

  return (
    <DropdownMenuItem
      onMouseEnter={onHover}
      onSelect={onSelect}
      className={classes.trim()}
    >
      <div className="flex items-center justify-between w-full gap-2">
        <span className="truncate flex-1 min-w-0">{group.name}</span>
        {group.base_branch && (
          <span className="flex items-center gap-1 text-xs bg-background px-1 rounded text-muted-foreground flex-shrink-0">
            <GitBranch className="h-3 w-3" />
            <span className="truncate max-w-[100px]">{group.base_branch}</span>
          </span>
        )}
      </div>
    </DropdownMenuItem>
  );
});

function GroupSelector({
  groups,
  selectedGroupId,
  onGroupSelect,
  placeholder,
  allowNone = false,
  allowAll = false,
  className = '',
  disabled = false,
}: GroupSelectorProps) {
  const { t } = useTranslation(['common', 'tasks']);
  const [searchTerm, setSearchTerm] = useState('');
  const [highlightedIndex, setHighlightedIndex] = useState<number | null>(null);
  const [open, setOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const virtuosoRef = useRef<VirtuosoHandle>(null);

  const effectivePlaceholder =
    placeholder ?? t('tasks:taskFormDialog.groupPlaceholder');

  // Build the list of items including special options
  const items = useMemo(() => {
    const result: Array<
      { type: 'all' } | { type: 'none' } | { type: 'group'; group: TaskGroup }
    > = [];

    if (allowAll) {
      result.push({ type: 'all' });
    }
    if (allowNone) {
      result.push({ type: 'none' });
    }

    const q = searchTerm.toLowerCase().trim();
    const filteredGroups = q
      ? groups.filter((g) => g.name.toLowerCase().includes(q))
      : groups;

    for (const group of filteredGroups) {
      result.push({ type: 'group', group });
    }

    return result;
  }, [groups, searchTerm, allowAll, allowNone]);

  const selectedGroupName = useMemo(() => {
    if (selectedGroupId === null) {
      if (allowAll) return t('tasks:taskFilterBar.allGroups');
      return null;
    }
    const group = groups.find((g) => g.id === selectedGroupId);
    return group?.name ?? null;
  }, [selectedGroupId, groups, allowAll, t]);

  const handleSelect = useCallback(
    (groupId: string | null) => {
      onGroupSelect(groupId);
      setSearchTerm('');
      setHighlightedIndex(null);
      setOpen(false);
    },
    [onGroupSelect]
  );

  // Reset highlight when filtered list changes
  useEffect(() => {
    if (highlightedIndex !== null && highlightedIndex >= items.length) {
      setHighlightedIndex(null);
    }
  }, [items, highlightedIndex]);

  useEffect(() => {
    setHighlightedIndex(null);
  }, [searchTerm]);

  const moveHighlight = useCallback(
    (delta: 1 | -1) => {
      if (items.length === 0) return;

      const start = highlightedIndex ?? -1;
      const next = (start + delta + items.length) % items.length;
      setHighlightedIndex(next);
      virtuosoRef.current?.scrollIntoView({
        index: next,
        behavior: 'auto',
      });
    },
    [items, highlightedIndex]
  );

  const attemptSelect = useCallback(() => {
    if (highlightedIndex == null) return;
    const item = items[highlightedIndex];
    if (!item) return;

    if (item.type === 'all') {
      handleSelect(null);
    } else if (item.type === 'none') {
      handleSelect(null);
    } else {
      handleSelect(item.group.id);
    }
  }, [highlightedIndex, items, handleSelect]);

  return (
    <DropdownMenu
      open={open}
      onOpenChange={(next) => {
        if (disabled) return;
        setOpen(next);
        if (!next) {
          setSearchTerm('');
          setHighlightedIndex(null);
        }
      }}
    >
      <DropdownMenuTrigger asChild disabled={disabled}>
        <Button
          variant="outline"
          size="sm"
          className={`w-full justify-between text-xs ${className}`}
          disabled={disabled}
        >
          <div className="flex items-center gap-1.5 w-full min-w-0">
            <Layers className="h-3 w-3 flex-shrink-0" />
            <span className="truncate">
              {selectedGroupName || effectivePlaceholder}
            </span>
          </div>
          <ArrowDown className="h-3 w-3 flex-shrink-0" />
        </Button>
      </DropdownMenuTrigger>

      <DropdownMenuContent className="w-72">
        <div className="p-2">
          <div className="relative">
            <Search className="absolute left-2 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              ref={searchInputRef}
              placeholder={t('tasks:groupSelector.searchPlaceholder')}
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              onKeyDown={(e) => {
                switch (e.key) {
                  case 'ArrowDown':
                    e.preventDefault();
                    e.stopPropagation();
                    moveHighlight(1);
                    return;
                  case 'ArrowUp':
                    e.preventDefault();
                    e.stopPropagation();
                    moveHighlight(-1);
                    return;
                  case 'Enter':
                    e.preventDefault();
                    e.stopPropagation();
                    attemptSelect();
                    return;
                  case 'Escape':
                    e.preventDefault();
                    e.stopPropagation();
                    setOpen(false);
                    return;
                  case 'Tab':
                    return;
                  default:
                    e.stopPropagation();
                }
              }}
              className="pl-8"
            />
          </div>
        </div>
        <DropdownMenuSeparator />

        {items.length === 0 ? (
          <div className="p-2 text-sm text-muted-foreground text-center">
            {t('tasks:groupSelector.empty')}
          </div>
        ) : (
          <Virtuoso
            ref={virtuosoRef}
            style={{ height: '12rem' }}
            totalCount={items.length}
            computeItemKey={(idx) => {
              const item = items[idx];
              if (item.type === 'all') return '__all__';
              if (item.type === 'none') return '__none__';
              return item.group.id;
            }}
            itemContent={(idx) => {
              const item = items[idx];
              const isHighlighted = idx === highlightedIndex;

              if (item.type === 'all') {
                const isSelected = selectedGroupId === null;
                return (
                  <DropdownMenuItem
                    onMouseEnter={() => setHighlightedIndex(idx)}
                    onSelect={() => handleSelect(null)}
                    className={
                      (isSelected ? 'bg-accent text-accent-foreground ' : '') +
                      (!isSelected && isHighlighted
                        ? 'bg-accent/70 ring-2 ring-accent '
                        : '') +
                      'transition-none'
                    }
                  >
                    {t('tasks:taskFilterBar.allGroups')}
                  </DropdownMenuItem>
                );
              }

              if (item.type === 'none') {
                const isSelected = selectedGroupId === null && !allowAll;
                return (
                  <DropdownMenuItem
                    onMouseEnter={() => setHighlightedIndex(idx)}
                    onSelect={() => handleSelect(null)}
                    className={
                      (isSelected ? 'bg-accent text-accent-foreground ' : '') +
                      (!isSelected && isHighlighted
                        ? 'bg-accent/70 ring-2 ring-accent '
                        : '') +
                      'transition-none'
                    }
                  >
                    {t('tasks:taskFormDialog.noGroup')}
                  </DropdownMenuItem>
                );
              }

              const isSelected = selectedGroupId === item.group.id;
              return (
                <GroupRow
                  group={item.group}
                  isSelected={isSelected}
                  isHighlighted={isHighlighted}
                  onHover={() => setHighlightedIndex(idx)}
                  onSelect={() => handleSelect(item.group.id)}
                />
              );
            }}
          />
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export default GroupSelector;
