import * as React from 'react';
import { ChevronDown, Check } from 'lucide-react';
import { Button, type ButtonProps } from './button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './dropdown-menu';
import { cn } from '@/lib/utils';

export type SplitButtonOption<T extends string = string> = {
  value: T;
  label: string;
  icon?: React.ReactNode;
  disabled?: boolean;
};

type SplitButtonProps<T extends string = string> = {
  options: SplitButtonOption<T>[];
  selectedValue: T;
  onSelect: (value: T) => void;
  onPrimaryClick: (value: T) => void;
  disabled?: boolean;
  loading?: boolean;
  loadingLabel?: string;
  successLabel?: string;
  showSuccess?: boolean;
  className?: string;
  variant?: ButtonProps['variant'];
  size?: ButtonProps['size'];
  icon?: React.ReactNode;
};

export function SplitButton<T extends string = string>({
  options,
  selectedValue,
  onSelect,
  onPrimaryClick,
  disabled,
  loading,
  loadingLabel,
  successLabel,
  showSuccess,
  className,
  variant = 'outline',
  size = 'xs',
  icon,
}: SplitButtonProps<T>) {
  const selectedOption = options.find((o) => o.value === selectedValue);
  const displayLabel = showSuccess
    ? successLabel
    : loading
      ? loadingLabel
      : selectedOption?.label;

  return (
    <div className={cn('inline-flex', className)}>
      <Button
        onClick={() => onPrimaryClick(selectedValue)}
        disabled={disabled || loading}
        variant={variant}
        size={size}
        className="rounded-r-none border-r-0 gap-1"
      >
        {icon}
        <span className="truncate max-w-[12ch]">{displayLabel}</span>
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant={variant}
            size={size}
            className="rounded-l-none px-1.5"
            disabled={disabled || loading}
          >
            <ChevronDown className="h-3.5 w-3.5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {options.map((option) => (
            <DropdownMenuItem
              key={option.value}
              onClick={() => onSelect(option.value)}
              className="gap-2"
              disabled={option.disabled}
            >
              {option.icon}
              <span className="flex-1">{option.label}</span>
              {option.value === selectedValue && (
                <Check className="h-4 w-4 text-muted-foreground" />
              )}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
