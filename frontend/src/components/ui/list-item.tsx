import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';
import { Text } from './text';

const listItemVariants = cva(
  'flex items-center gap-3 px-3 py-2 transition-colors',
  {
    variants: {
      variant: {
        default: '',
        interactive: 'cursor-pointer hover:bg-accent',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);

export interface ListItemProps
  extends Omit<React.HTMLAttributes<HTMLDivElement>, 'title'>,
    VariantProps<typeof listItemVariants> {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  icon?: React.ReactNode;
  actions?: React.ReactNode;
}

const ListItem = React.forwardRef<HTMLDivElement, ListItemProps>(
  (
    { className, variant, title, subtitle, icon, actions, onClick, ...props },
    ref
  ) => {
    return (
      <div
        ref={ref}
        className={cn(listItemVariants({ variant, className }))}
        onClick={onClick}
        {...props}
      >
        {icon && (
          <div className="flex-shrink-0 w-5 flex items-center justify-center text-muted-foreground">
            {icon}
          </div>
        )}
        <div className="flex-1 min-w-0">
          <Text size="sm" className="truncate block">
            {title}
          </Text>
          {subtitle && (
            <Text size="xs" variant="secondary" className="truncate block">
              {subtitle}
            </Text>
          )}
        </div>
        {actions && <div className="flex-shrink-0">{actions}</div>}
      </div>
    );
  }
);
ListItem.displayName = 'ListItem';

export { ListItem, listItemVariants };
