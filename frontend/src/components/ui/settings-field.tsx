import * as React from 'react';

import { cn } from '@/lib/utils';
import { Label } from '@/components/ui/label';
import { Text } from '@/components/ui/text';

export interface SettingsFieldProps
  extends React.HTMLAttributes<HTMLDivElement> {
  label?: string;
  htmlFor?: string;
  description?: React.ReactNode;
  error?: string | null;
  children: React.ReactNode;
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

function SettingsField({
  label,
  htmlFor,
  description,
  error,
  layout = 'vertical',
  indent = false,
  className,
  children,
  ...props
}: SettingsFieldProps) {
  if (layout === 'horizontal') {
    return (
      <div
        className={cn(
          'flex items-start gap-3',
          indent && 'ml-6',
          className
        )}
        {...props}
      >
        {children}
        <div className="space-y-0.5">
          {label && htmlFor && (
            <Label htmlFor={htmlFor} className="cursor-pointer">
              {label}
            </Label>
          )}
          {label && !htmlFor && (
            <Text size="sm" className="font-medium">
              {label}
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

  return (
    <div
      className={cn('space-y-2', indent && 'ml-6', className)}
      {...props}
    >
      {label && htmlFor && <Label htmlFor={htmlFor}>{label}</Label>}
      {label && !htmlFor && (
        <Text size="sm" className="font-medium">
          {label}
        </Text>
      )}
      {children}
      {error && (
        <Text size="sm" as="p" className="text-destructive">
          {error}
        </Text>
      )}
      {description && (
        <Text variant="secondary" size="sm" as="p">
          {description}
        </Text>
      )}
    </div>
  );
}

export { SettingsField };
