import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';

const cardVariants = cva('bg-card text-card-foreground', {
  variants: {
    variant: {
      default: '',
      elevated: 'shadow-low',
      outlined: 'border border-border',
      ghost: 'bg-transparent',
    },
    spacing: {
      comfortable: '',
      compact: '',
    },
  },
  defaultVariants: {
    variant: 'default',
    spacing: 'comfortable',
  },
});

type CardSpacing = 'comfortable' | 'compact';

const CardSpacingContext = React.createContext<CardSpacing>('comfortable');

export interface CardProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof cardVariants> {}

const Card = React.forwardRef<HTMLDivElement, CardProps>(
  ({ className, variant, spacing = 'comfortable', ...props }, ref) => (
    <CardSpacingContext.Provider value={spacing ?? 'comfortable'}>
      <div
        ref={ref}
        className={cn(cardVariants({ variant, spacing, className }))}
        {...props}
      />
    </CardSpacingContext.Provider>
  )
);
Card.displayName = 'Card';

const CardHeader = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => {
  const spacing = React.useContext(CardSpacingContext);
  return (
    <div
      ref={ref}
      className={cn(
        'flex flex-col space-y-1.5',
        spacing === 'compact' ? 'p-4' : 'p-6',
        className
      )}
      {...props}
    />
  );
});
CardHeader.displayName = 'CardHeader';

const CardTitle = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(
      'text-2xl font-semibold leading-none tracking-tight',
      className
    )}
    {...props}
  />
));
CardTitle.displayName = 'CardTitle';

const CardDescription = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn('text-sm text-muted-foreground', className)}
    {...props}
  />
));
CardDescription.displayName = 'CardDescription';

const CardContent = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => {
  const spacing = React.useContext(CardSpacingContext);
  return (
    <div
      ref={ref}
      className={cn(
        spacing === 'compact' ? 'p-4 pt-0' : 'p-6 pt-0',
        className
      )}
      {...props}
    />
  );
});
CardContent.displayName = 'CardContent';

const CardFooter = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => {
  const spacing = React.useContext(CardSpacingContext);
  return (
    <div
      ref={ref}
      className={cn(
        'flex items-center',
        spacing === 'compact' ? 'p-4 pt-0' : 'p-6 pt-0',
        className
      )}
      {...props}
    />
  );
});
CardFooter.displayName = 'CardFooter';

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardDescription,
  CardContent,
  cardVariants,
};
