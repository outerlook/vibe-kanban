import * as React from 'react';

import { cn } from '@/lib/utils';
import { Label } from '@/components/ui/label';
import { Text } from '@/components/ui/text';

export interface SettingsFieldProps
  extends React.HTMLAttributes<HTMLDivElement> {
  /** Label for the field */
  label: string;
  /** Optional helper text shown below the input */
  helper?: React.ReactNode;
  /** Optional error message */
  error?: string;
  /** HTML id for the form control (used to link label) */
  htmlFor?: string;
  /** Whether the field is required */
  required?: boolean;
  /** Layout orientation */
  orientation?: 'vertical' | 'horizontal';
}

const SettingsField = React.forwardRef<HTMLDivElement, SettingsFieldProps>(
  (
    {
      className,
      label,
      helper,
      error,
      htmlFor,
      required,
      orientation = 'vertical',
      children,
      ...props
    },
    ref
  ) => {
    const isHorizontal = orientation === 'horizontal';

    return (
      <div
        ref={ref}
        className={cn(
          isHorizontal
            ? 'flex items-start gap-4'
            : 'space-y-2',
          className
        )}
        {...props}
      >
        <div className={cn(isHorizontal && 'flex-shrink-0 pt-2.5 w-32')}>
          <Label
            htmlFor={htmlFor}
            className={cn(error && 'text-destructive')}
          >
            {label}
            {required && <span className="text-destructive ml-1">*</span>}
          </Label>
        </div>
        <div className={cn('space-y-1.5', isHorizontal && 'flex-1')}>
          {children}
          {error && (
            <Text size="sm" className="text-destructive">
              {error}
            </Text>
          )}
          {helper && !error && (
            <Text variant="secondary" size="sm" as="p">
              {helper}
            </Text>
          )}
        </div>
      </div>
    );
  }
);
SettingsField.displayName = 'SettingsField';

export { SettingsField };
