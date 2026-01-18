import * as React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Section, type SectionProps } from '@/components/ui/section';
import { Badge, type BadgeProps } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

export interface SettingsSectionProps
  extends Omit<SectionProps, 'actions' | 'variant'> {
  id: string;
  collapsible?: boolean;
  defaultExpanded?: boolean;
  badge?: {
    label: string;
    variant?: BadgeProps['variant'];
  };
  actions?: React.ReactNode;
}

function getStorageKey(id: string): string {
  return `settings-section-${id}-expanded`;
}

function getInitialExpanded(id: string, defaultExpanded: boolean): boolean {
  if (typeof window === 'undefined') return defaultExpanded;

  const stored = localStorage.getItem(getStorageKey(id));
  if (stored === null) return defaultExpanded;

  return stored === 'true';
}

export function SettingsSection({
  id,
  title,
  description,
  children,
  collapsible = false,
  defaultExpanded = true,
  badge,
  actions,
  className,
  headingLevel = 'h3',
  spacing,
  ...props
}: SettingsSectionProps) {
  const [isExpanded, setIsExpanded] = React.useState(() =>
    collapsible ? getInitialExpanded(id, defaultExpanded) : true
  );

  const contentId = React.useId();

  const handleToggle = React.useCallback(() => {
    if (!collapsible) return;

    setIsExpanded((prev) => {
      const next = !prev;
      localStorage.setItem(getStorageKey(id), String(next));
      return next;
    });
  }, [collapsible, id]);

  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent) => {
      if (!collapsible) return;
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        handleToggle();
      }
    },
    [collapsible, handleToggle]
  );

  const headerActions = (
    <div className="flex items-center gap-2">
      {badge && <Badge variant={badge.variant}>{badge.label}</Badge>}
      {actions}
      {collapsible && (
        <button
          type="button"
          onClick={handleToggle}
          onKeyDown={handleKeyDown}
          aria-expanded={isExpanded}
          aria-controls={contentId}
          className={cn(
            'p-1 rounded-md transition-colors',
            'hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
          )}
        >
          {isExpanded ? (
            <ChevronDown className="h-4 w-4" />
          ) : (
            <ChevronRight className="h-4 w-4" />
          )}
          <span className="sr-only">
            {isExpanded ? 'Collapse section' : 'Expand section'}
          </span>
        </button>
      )}
    </div>
  );

  return (
    <Section
      title={title}
      description={description}
      actions={headerActions}
      variant="card"
      headingLevel={headingLevel}
      spacing={spacing}
      className={className}
      {...props}
    >
      <div
        id={contentId}
        className={cn(
          'grid transition-[grid-template-rows] duration-200 ease-out',
          isExpanded ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'
        )}
      >
        <div className="overflow-hidden">{children}</div>
      </div>
    </Section>
  );
}
