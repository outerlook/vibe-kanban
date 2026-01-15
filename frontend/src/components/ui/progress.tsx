import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';

const progressVariants = cva('h-2 rounded-full transition-all', {
  variants: {
    variant: {
      default: 'bg-primary',
      warning: 'bg-yellow-500',
      danger: 'bg-destructive',
    },
  },
  defaultVariants: {
    variant: 'default',
  },
});

function getAutoVariant(value: number): 'default' | 'warning' | 'danger' {
  if (value > 85) return 'danger';
  if (value >= 60) return 'warning';
  return 'default';
}

export interface ProgressProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof progressVariants> {
  value: number;
}

const Progress = React.forwardRef<HTMLDivElement, ProgressProps>(
  ({ className, value, variant, ...props }, ref) => {
    const clampedValue = Math.max(0, Math.min(100, value));
    const resolvedVariant = variant ?? getAutoVariant(clampedValue);

    return (
      <div
        ref={ref}
        role="progressbar"
        aria-valuenow={clampedValue}
        aria-valuemin={0}
        aria-valuemax={100}
        className={cn('h-2 w-full overflow-hidden rounded-full bg-muted', className)}
        {...props}
      >
        <div
          className={cn(progressVariants({ variant: resolvedVariant }))}
          style={{ width: `${clampedValue}%` }}
        />
      </div>
    );
  }
);
Progress.displayName = 'Progress';

export { Progress, progressVariants };
