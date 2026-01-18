import * as React from 'react';
import { Label } from '@/components/ui/label';
import { Text } from '@/components/ui/text';
import { cn } from '@/lib/utils';

export interface SettingsFieldProps {
  label: string;
  description?: string;
  error?: string;
  required?: boolean;
  children: React.ReactNode;
  className?: string;
  htmlFor?: string;
}

export function SettingsField({
  label,
  description,
  error,
  required,
  children,
  className,
  htmlFor,
}: SettingsFieldProps) {
  return (
    <div className={cn('space-y-2', className)}>
      <div className="space-y-1">
        <Label htmlFor={htmlFor}>
          {label}
          {required && (
            <span className="ml-1 text-destructive" aria-hidden="true">
              *
            </span>
          )}
        </Label>
        {description && (
          <Text variant="secondary" size="sm" as="p">
            {description}
          </Text>
        )}
      </div>
      {children}
      {error && (
        <Text size="sm" as="p" className="text-destructive">
          {error}
        </Text>
      )}
    </div>
  );
}
