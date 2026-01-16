import { Search } from 'lucide-react';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';

export interface PrFiltersProps {
  branches: string[];
  selectedBranch: string | null;
  searchQuery: string;
  onBranchChange: (branch: string | null) => void;
  onSearchChange: (query: string) => void;
  className?: string;
}

const ALL_BRANCHES_VALUE = '__all__';

export function PrFilters({
  branches,
  selectedBranch,
  searchQuery,
  onBranchChange,
  onSearchChange,
  className,
}: PrFiltersProps) {
  const handleBranchChange = (value: string) => {
    onBranchChange(value === ALL_BRANCHES_VALUE ? null : value);
  };

  return (
    <div className={cn('flex items-center gap-3 flex-wrap', className)}>
      {/* Base branch filter */}
      <Select
        value={selectedBranch ?? ALL_BRANCHES_VALUE}
        onValueChange={handleBranchChange}
      >
        <SelectTrigger className="w-[180px]">
          <SelectValue placeholder="Base branch" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={ALL_BRANCHES_VALUE}>All branches</SelectItem>
          {branches.map((branch) => (
            <SelectItem key={branch} value={branch}>
              {branch}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* Search input */}
      <div className="relative flex-1 min-w-[200px]">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
        <Input
          type="text"
          placeholder="Search PRs by title..."
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          className="pl-9"
        />
      </div>
    </div>
  );
}

export function PrFiltersSkeleton() {
  return (
    <div className="flex items-center gap-3 animate-pulse">
      <div className="w-[180px] h-10 bg-muted rounded" />
      <div className="flex-1 min-w-[200px] h-10 bg-muted rounded" />
    </div>
  );
}
