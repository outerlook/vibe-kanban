import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';
import { Card } from '@/components/ui/card';
import { Text } from '@/components/ui/text';

const sectionVariants = cva('', {
  variants: {
    spacing: {
      comfortable: 'space-y-6',
      compact: 'space-y-4',
    },
  },
  defaultVariants: {
    spacing: 'comfortable',
  },
});

export interface SectionProps
  extends React.HTMLAttributes<HTMLElement>,
    VariantProps<typeof sectionVariants> {
  title: string;
  description?: string;
  actions?: React.ReactNode;
  variant?: 'card' | 'plain';
  /**
   * The heading level for the section title.
   * Defaults to 'h2' for proper semantic hierarchy.
   */
  headingLevel?: 'h2' | 'h3' | 'h4';
}

const Section = React.forwardRef<HTMLElement, SectionProps>(
  (
    {
      className,
      title,
      description,
      actions,
      variant = 'plain',
      spacing,
      headingLevel: Heading = 'h2',
      children,
      ...props
    },
    ref
  ) => {
    const titleId = React.useId();

    const header = (
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <Heading
            id={titleId}
            className="text-lg font-semibold leading-none tracking-tight"
          >
            {title}
          </Heading>
          {description && (
            <Text variant="secondary" size="sm" as="p">
              {description}
            </Text>
          )}
        </div>
        {actions && <div className="flex-shrink-0">{actions}</div>}
      </div>
    );

    const content = (
      <section
        ref={ref}
        aria-labelledby={titleId}
        className={cn(sectionVariants({ spacing, className }))}
        {...props}
      >
        {header}
        {children && <div>{children}</div>}
      </section>
    );

    if (variant === 'card') {
      return <Card className="p-6">{content}</Card>;
    }

    return content;
  }
);
Section.displayName = 'Section';

export { Section, sectionVariants };
