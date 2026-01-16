import { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { PrCard, PrCardSkeleton, type PrData } from './PrCard';
import { cn } from '@/lib/utils';

export interface RepoSectionProps {
  repoName: string;
  prs: PrData[];
  defaultOpen?: boolean;
  className?: string;
}

export function RepoSection({
  repoName,
  prs,
  defaultOpen = true,
  className,
}: RepoSectionProps) {
  const [isOpen, setIsOpen] = useState(defaultOpen);

  const toggleOpen = () => setIsOpen(!isOpen);

  if (prs.length === 0) {
    return null;
  }

  return (
    <div className={cn('border border-border rounded-md', className)}>
      {/* Collapsible header */}
      <Button
        variant="ghost"
        className="w-full justify-start gap-2 px-3 py-2 h-auto font-medium"
        onClick={toggleOpen}
      >
        {isOpen ? (
          <ChevronDown className="w-4 h-4 flex-shrink-0" />
        ) : (
          <ChevronRight className="w-4 h-4 flex-shrink-0" />
        )}
        <span className="truncate">{repoName}</span>
        <span className="text-muted-foreground text-xs ml-auto">
          {prs.length} PR{prs.length !== 1 ? 's' : ''}
        </span>
      </Button>

      {/* Collapsible content */}
      {isOpen && (
        <div className="px-3 pb-3 space-y-2">
          {prs.map((pr) => (
            <PrCard key={pr.id} pr={pr} />
          ))}
        </div>
      )}
    </div>
  );
}

export function RepoSectionSkeleton({ prCount = 2 }: { prCount?: number }) {
  return (
    <div className="border border-border rounded-md animate-pulse">
      {/* Header skeleton */}
      <div className="px-3 py-2 flex items-center gap-2">
        <div className="w-4 h-4 bg-muted rounded" />
        <div className="h-4 bg-muted rounded w-32" />
        <div className="h-3 bg-muted rounded w-12 ml-auto" />
      </div>

      {/* Content skeleton */}
      <div className="px-3 pb-3 space-y-2">
        {Array.from({ length: prCount }).map((_, i) => (
          <PrCardSkeleton key={i} />
        ))}
      </div>
    </div>
  );
}
