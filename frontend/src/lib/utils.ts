import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatFileSize(bytes: bigint | null | undefined): string {
  if (!bytes) return '';
  const num = Number(bytes);
  if (num < 1024) return `${num} B`;
  if (num < 1024 * 1024) return `${(num / 1024).toFixed(1)} KB`;
  return `${(num / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatTokenCount(n: bigint | number | null | undefined): string {
  if (n == null) return '';
  const num = Number(n);
  if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
  if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
  return num.toString();
}

/**
 * Format an ISO date string as a short date (e.g., "Jan 15, 2025").
 * Returns the original string on parse failure.
 */
export function formatShortDate(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
  } catch {
    return dateStr;
  }
}

/**
 * Format an ISO date string as a full date-time (e.g., "1/15/2025, 10:30:00 AM").
 * Returns the original string on parse failure.
 */
export function formatDateTime(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleString();
  } catch {
    return dateStr;
  }
}

/**
 * Format an ISO date string as a human-readable relative time (e.g., "5 minutes ago").
 * Uses Intl.RelativeTimeFormat when available for localized output.
 */
export function formatRelativeTime(iso: string): string {
  const d = new Date(iso);
  const diffMs = Date.now() - d.getTime();
  const absSec = Math.round(Math.abs(diffMs) / 1000);

  const rtf =
    typeof Intl !== 'undefined' &&
    typeof Intl.RelativeTimeFormat === 'function'
      ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
      : null;

  const to = (value: number, unit: Intl.RelativeTimeFormatUnit) =>
    rtf
      ? rtf.format(-value, unit)
      : `${value} ${unit}${value !== 1 ? 's' : ''} ago`;

  if (absSec < 60) return to(Math.round(absSec), 'second');
  const mins = Math.round(absSec / 60);
  if (mins < 60) return to(mins, 'minute');
  const hours = Math.round(mins / 60);
  if (hours < 24) return to(hours, 'hour');
  const days = Math.round(hours / 24);
  if (days < 30) return to(days, 'day');
  const months = Math.round(days / 30);
  if (months < 12) return to(months, 'month');
  const years = Math.round(months / 12);
  return to(years, 'year');
}
