import * as React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';

import { cn } from '@/lib/utils';
import { Card } from '@/components/ui/card';
import { Text } from '@/components/ui/text';

export interface SettingsSectionProps
  extends React.HTMLAttributes<HTMLElement> {
  /** Section title */
  title: string;
  /** Optional section description */
  description?: string;
  /** Optional actions rendered in the header */
  actions?: React.ReactNode;
  /** Whether the section is collapsible */
  collapsible?: boolean;
  /** Initial collapsed state (only used when collapsible=true) */
  defaultCollapsed?: boolean;
  /** Optional ID for deep-linking */
  sectionId?: string;
}

const SettingsSection = React.forwardRef<HTMLElement, SettingsSectionProps>(
  (
    {
      className,
      title,
      description,
      actions,
      collapsible = false,
      defaultCollapsed = false,
      sectionId,
      children,
      ...props
    },
    ref
  ) => {
    const [isCollapsed, setIsCollapsed] = React.useState(defaultCollapsed);
    const titleId = React.useId();
    const contentId = React.useId();

    const toggleCollapse = () => {
      if (collapsible) {
        setIsCollapsed((prev) => !prev);
      }
    };

    const headerContent = (
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1 flex-1">
          <div className="flex items-center gap-2">
            {collapsible && (
              <button
                type="button"
                onClick={toggleCollapse}
                className="p-0.5 -ml-1 rounded hover:bg-muted transition-colors"
                aria-expanded={!isCollapsed}
                aria-controls={contentId}
              >
                {isCollapsed ? (
                  <ChevronRight className="h-4 w-4 text-muted-foreground" />
                ) : (
                  <ChevronDown className="h-4 w-4 text-muted-foreground" />
                )}
              </button>
            )}
            <h2
              id={titleId}
              className={cn(
                'text-lg font-semibold leading-none tracking-tight',
                collapsible && 'cursor-pointer select-none'
              )}
              onClick={collapsible ? toggleCollapse : undefined}
            >
              {title}
            </h2>
          </div>
          {description && !isCollapsed && (
            <Text variant="secondary" size="sm" as="p" className="pt-1">
              {description}
            </Text>
          )}
        </div>
        {actions && !isCollapsed && (
          <div className="flex-shrink-0">{actions}</div>
        )}
      </div>
    );

    return (
      <Card
        id={sectionId}
        className={cn('overflow-hidden', className)}
        {...props}
      >
        <section ref={ref} aria-labelledby={titleId}>
          <div className="p-6 pb-0">{headerContent}</div>
          <div
            id={contentId}
            className={cn(
              'transition-all duration-200 ease-in-out',
              isCollapsed
                ? 'max-h-0 opacity-0 overflow-hidden pb-0'
                : 'max-h-none opacity-100 p-6 pt-4'
            )}
            aria-hidden={isCollapsed}
          >
            <div className="space-y-4">{children}</div>
          </div>
        </section>
      </Card>
    );
  }
);
SettingsSection.displayName = 'SettingsSection';

export { SettingsSection };
