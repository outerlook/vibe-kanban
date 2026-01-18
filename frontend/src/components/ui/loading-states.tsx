import * as React from 'react';

import { cn } from '@/lib/utils';
import {
  Card,
  CardContent,
  CardHeader,
} from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';

export interface SkeletonCardProps
  extends React.HTMLAttributes<HTMLDivElement> {
  showDescription?: boolean;
  showContent?: boolean;
}

function SkeletonCard({
  className,
  showDescription = true,
  showContent = true,
  ...props
}: SkeletonCardProps) {
  return (
    <Card variant="ghost" className={cn(className)} {...props}>
      <CardHeader>
        <Skeleton variant="text" className="h-6 w-3/4" />
        {showDescription && (
          <Skeleton variant="text" className="h-4 w-1/2" />
        )}
      </CardHeader>
      {showContent && (
        <CardContent className="space-y-2">
          <Skeleton variant="text" className="h-4 w-full" />
          <Skeleton variant="text" className="h-4 w-5/6" />
          <Skeleton variant="text" className="h-4 w-4/6" />
        </CardContent>
      )}
    </Card>
  );
}

export interface SkeletonTableProps
  extends React.HTMLAttributes<HTMLDivElement> {
  rows?: number;
  columns?: number;
}

function SkeletonTable({
  className,
  rows = 5,
  columns = 4,
  ...props
}: SkeletonTableProps) {
  return (
    <div className={cn('w-full', className)} {...props}>
      {/* Header */}
      <div className="flex gap-4 border-b border-border pb-3 mb-3">
        {Array.from({ length: columns }).map((_, i) => (
          <Skeleton
            key={`header-${i}`}
            variant="text"
            className="h-4 flex-1"
          />
        ))}
      </div>
      {/* Rows */}
      <div className="space-y-3">
        {Array.from({ length: rows }).map((_, rowIndex) => (
          <div key={`row-${rowIndex}`} className="flex gap-4">
            {Array.from({ length: columns }).map((_, colIndex) => (
              <Skeleton
                key={`cell-${rowIndex}-${colIndex}`}
                variant="text"
                className="h-4 flex-1"
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

export interface SkeletonListProps
  extends React.HTMLAttributes<HTMLDivElement> {
  items?: number;
  showIcon?: boolean;
  showSecondaryText?: boolean;
}

function SkeletonList({
  className,
  items = 3,
  showIcon = true,
  showSecondaryText = true,
  ...props
}: SkeletonListProps) {
  return (
    <div className={cn('space-y-3', className)} {...props}>
      {Array.from({ length: items }).map((_, i) => (
        <div key={i} className="flex items-center gap-3">
          {showIcon && (
            <Skeleton variant="circular" className="h-10 w-10 shrink-0" />
          )}
          <div className="flex-1 space-y-1.5">
            <Skeleton variant="text" className="h-4 w-3/4" />
            {showSecondaryText && (
              <Skeleton variant="text" className="h-3 w-1/2" />
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

export interface SkeletonFormProps
  extends React.HTMLAttributes<HTMLDivElement> {
  fields?: number;
}

function SkeletonForm({
  className,
  fields = 3,
  ...props
}: SkeletonFormProps) {
  return (
    <div className={cn('space-y-4', className)} {...props}>
      {Array.from({ length: fields }).map((_, i) => (
        <div key={i} className="space-y-2">
          <Skeleton variant="text" className="h-4 w-24" />
          <Skeleton variant="rectangular" className="h-10 w-full" />
        </div>
      ))}
    </div>
  );
}

export { SkeletonCard, SkeletonTable, SkeletonList, SkeletonForm };
