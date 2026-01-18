import * as React from 'react';
import { Label } from '@/components/ui/label';
import { Text } from '@/components/ui/text';
import { cn } from '@/lib/utils';

export interface SettingsFieldProps {
  label?: string;
  description?: React.ReactNode;
  error?: string | null;
  required?: boolean;
  children: React.ReactNode;
  className?: string;
  htmlFor?: string;
  /**
   * Layout direction for the field.
   * - 'vertical': Label above input (default for text inputs)
   * - 'horizontal': Checkbox/switch style with label beside control
   */
  layout?: 'vertical' | 'horizontal';
  /**
   * If true, indents the field (useful for nested/conditional fields)
   */
  indent?: boolean;
}

export function SettingsField({
  label,
  description,
  error,
  required,
  children,
  className,
  htmlFor,
  layout = 'vertical',
  indent = false,
}: SettingsFieldProps) {
  // Horizontal layout: checkbox/switch beside label
  if (layout === 'horizontal') {
    return (
      <div
        className={cn('flex items-start gap-3', indent && 'ml-6', className)}
      >
        {children}
        <div className="space-y-0.5">
          {label && htmlFor && (
            <Label htmlFor={htmlFor} className="cursor-pointer">
              {label}
              {required && (
                <span className="ml-1 text-destructive" aria-hidden="true">
                  *
                </span>
              )}
            </Label>
          )}
          {label && !htmlFor && (
            <Text size="sm" className="font-medium">
              {label}
              {required && (
                <span className="ml-1 text-destructive" aria-hidden="true">
                  *
                </span>
              )}
            </Text>
          )}
          {description && (
            <Text variant="secondary" size="sm" as="p">
              {description}
            </Text>
          )}
          {error && (
            <Text size="sm" as="p" className="text-destructive">
              {error}
            </Text>
          )}
        </div>
      </div>
    );
  }

  // Vertical layout (default): label above input
  return (
    <div className={cn('space-y-2', indent && 'ml-6', className)}>
      {label && (
        <div className="space-y-1">
          {htmlFor ? (
            <Label htmlFor={htmlFor}>
              {label}
              {required && (
                <span className="ml-1 text-destructive" aria-hidden="true">
                  *
                </span>
              )}
            </Label>
          ) : (
            <Text size="sm" className="font-medium">
              {label}
              {required && (
                <span className="ml-1 text-destructive" aria-hidden="true">
                  *
                </span>
              )}
            </Text>
          )}
          {description && (
            <Text variant="secondary" size="sm" as="p">
              {description}
            </Text>
          )}
        </div>
      )}
      {!label && description && (
        <Text variant="secondary" size="sm" as="p">
          {description}
        </Text>
      )}
      {children}
      {error && (
        <Text size="sm" as="p" className="text-destructive">
          {error}
        </Text>
      )}
    </div>
  );
}
