import {
  GitPullRequest,
  MessageSquare,
  ExternalLink,
  Loader2,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

export interface PrData {
  id: string | number;
  title: string;
  url: string;
  author: string;
  baseBranch: string;
  headBranch: string;
  unresolvedComments: number | null;
  createdAt: string;
}

export interface PrCardProps {
  pr: PrData;
  className?: string;
  onClick?: () => void;
  selected?: boolean;
}

function formatDate(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
  } catch {
    return dateStr;
  }
}

export function PrCard({ pr, className, onClick, selected }: PrCardProps) {
  const isCountLoading = pr.unresolvedComments === null;
  const hasUnresolved =
    pr.unresolvedComments !== null && pr.unresolvedComments > 0;

  return (
    <div
      className={cn(
        'p-3 bg-muted/50 rounded-md border border-border hover:border-muted-foreground transition-colors',
        onClick && 'cursor-pointer',
        selected && 'ring-2 ring-primary bg-primary/5',
        className
      )}
      onClick={onClick}
    >
      {/* Header with title and external link */}
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-start gap-2 min-w-0 flex-1">
          <GitPullRequest className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
          <a
            href={pr.url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-sm font-medium hover:underline truncate"
            title={pr.title}
            onClick={(e) => e.stopPropagation()}
          >
            {pr.title}
          </a>
        </div>
        <a
          href={pr.url}
          target="_blank"
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-foreground transition-colors flex-shrink-0"
          aria-label="Open in GitHub"
          onClick={(e) => e.stopPropagation()}
        >
          <ExternalLink className="w-3.5 h-3.5" />
        </a>
      </div>

      {/* Metadata row */}
      <div className="mt-2 flex items-center gap-3 text-xs text-muted-foreground flex-wrap">
        {/* Author */}
        <span>@{pr.author}</span>

        {/* Branch info */}
        <span className="font-mono text-xs">
          {pr.baseBranch}
          <span className="mx-1">&larr;</span>
          {pr.headBranch}
        </span>

        {/* Created date */}
        <span>{formatDate(pr.createdAt)}</span>
      </div>

      {/* Unresolved comments badge */}
      <div className="mt-2 flex items-center gap-2">
        <Badge
          variant={hasUnresolved ? 'destructive' : 'secondary'}
          className="text-xs flex items-center gap-1"
        >
          <MessageSquare className="w-3 h-3" />
          {isCountLoading ? (
            <Loader2 className="w-3 h-3 animate-spin" />
          ) : (
            <>{pr.unresolvedComments} unresolved</>
          )}
        </Badge>
      </div>
    </div>
  );
}

export function PrCardSkeleton() {
  return (
    <div className="p-3 bg-muted/50 rounded-md border border-border animate-pulse">
      {/* Title skeleton */}
      <div className="flex items-start gap-2">
        <div className="w-4 h-4 bg-muted rounded flex-shrink-0" />
        <div className="h-4 bg-muted rounded w-3/4" />
      </div>

      {/* Metadata skeleton */}
      <div className="mt-2 flex items-center gap-3">
        <div className="h-3 bg-muted rounded w-16" />
        <div className="h-3 bg-muted rounded w-24" />
        <div className="h-3 bg-muted rounded w-20" />
      </div>

      {/* Badge skeleton */}
      <div className="mt-2">
        <div className="h-5 bg-muted rounded w-24" />
      </div>
    </div>
  );
}
