import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';
import { Button, ButtonProps } from './button';

const emptyStateVariants = cva(
  'flex flex-col items-center text-center',
  {
    variants: {
      size: {
        sm: 'py-8 gap-3',
        md: 'py-12 gap-4',
        lg: 'py-16 gap-5',
      },
    },
    defaultVariants: {
      size: 'md',
    },
  }
);

const iconContainerVariants = cva(
  'flex items-center justify-center rounded-lg bg-muted',
  {
    variants: {
      size: {
        sm: 'h-10 w-10',
        md: 'h-12 w-12',
        lg: 'h-14 w-14',
      },
    },
    defaultVariants: {
      size: 'md',
    },
  }
);

const iconVariants = cva('text-muted-foreground', {
  variants: {
    size: {
      sm: 'h-5 w-5',
      md: 'h-6 w-6',
      lg: 'h-7 w-7',
    },
  },
  defaultVariants: {
    size: 'md',
  },
});

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
  variant?: ButtonProps['variant'];
  icon?: React.ReactNode;
}

export interface EmptyStateProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof emptyStateVariants> {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: EmptyStateAction;
  secondaryAction?: EmptyStateAction;
}

const EmptyState = React.forwardRef<HTMLDivElement, EmptyStateProps>(
  (
    {
      className,
      size,
      icon,
      title,
      description,
      action,
      secondaryAction,
      ...props
    },
    ref
  ) => {
    return (
      <div
        ref={ref}
        className={cn(emptyStateVariants({ size, className }))}
        {...props}
      >
        {icon && (
          <div className={cn(iconContainerVariants({ size }))}>
            <div className={cn(iconVariants({ size }))}>
              {icon}
            </div>
          </div>
        )}
        <div className="space-y-1">
          <h3 className="text-lg font-semibold">{title}</h3>
          {description && (
            <p className="text-sm text-muted-foreground max-w-sm">
              {description}
            </p>
          )}
        </div>
        {(action || secondaryAction) && (
          <div className="flex items-center gap-2 mt-2">
            {action && (
              <Button
                variant={action.variant}
                onClick={action.onClick}
              >
                {action.icon}
                {action.label}
              </Button>
            )}
            {secondaryAction && (
              <Button
                variant={secondaryAction.variant ?? 'outline'}
                onClick={secondaryAction.onClick}
              >
                {secondaryAction.icon}
                {secondaryAction.label}
              </Button>
            )}
          </div>
        )}
      </div>
    );
  }
);
EmptyState.displayName = 'EmptyState';

export { EmptyState, emptyStateVariants };
