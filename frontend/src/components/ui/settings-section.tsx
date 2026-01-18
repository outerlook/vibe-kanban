import * as React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';

import { cn } from '@/lib/utils';
import { Card } from '@/components/ui/card';
import { Text } from '@/components/ui/text';

export interface SettingsSectionProps
  extends Omit<React.HTMLAttributes<HTMLElement>, 'title'> {
  id: string;
  title: string;
  description?: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}

function SettingsSection({
  id,
  title,
  description,
  defaultOpen = false,
  className,
  children,
  ...props
}: SettingsSectionProps) {
  const storageKey = `settings-section-${id}`;
  const [isOpen, setIsOpen] = React.useState(() => {
    if (typeof window === 'undefined') return defaultOpen;
    const stored = localStorage.getItem(storageKey);
    return stored !== null ? stored === 'true' : defaultOpen;
  });

  const toggleOpen = React.useCallback(() => {
    setIsOpen((prev) => {
      const next = !prev;
      localStorage.setItem(storageKey, String(next));
      return next;
    });
  }, [storageKey]);

  const titleId = React.useId();

  return (
    <Card
      variant="outlined"
      className={cn('overflow-hidden', className)}
      {...props}
    >
      <button
        type="button"
        onClick={toggleOpen}
        className="flex w-full items-start justify-between gap-4 p-6 text-left hover:bg-muted/50 transition-colors"
        aria-expanded={isOpen}
        aria-controls={`${id}-content`}
      >
        <div className="space-y-1">
          <h2
            id={titleId}
            className="text-lg font-semibold leading-none tracking-tight"
          >
            {title}
          </h2>
          {description && (
            <Text variant="secondary" size="sm" as="p">
              {description}
            </Text>
          )}
        </div>
        <div className="flex-shrink-0 mt-0.5">
          {isOpen ? (
            <ChevronDown className="h-5 w-5 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-5 w-5 text-muted-foreground" />
          )}
        </div>
      </button>
      {isOpen && (
        <section
          id={`${id}-content`}
          aria-labelledby={titleId}
          className="px-6 pb-6 pt-0 space-y-4"
        >
          {children}
        </section>
      )}
    </Card>
  );
}

export { SettingsSection };
