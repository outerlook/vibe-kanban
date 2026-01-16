import { Progress } from '@/components/ui/progress';

export interface UsageLimitDisplayProps {
  label: string;
  usedPercent: number;
  resetsAt: string;
}

function formatTimeUntilReset(resetsAt: string): string {
  const resetTime = new Date(resetsAt).getTime();
  const now = Date.now();
  const diffMs = resetTime - now;

  if (diffMs <= 0) {
    return 'resetting...';
  }

  const diffMinutes = Math.floor(diffMs / (1000 * 60));
  const diffHours = Math.floor(diffMinutes / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffDays > 0) {
    return `resets in ${diffDays} day${diffDays > 1 ? 's' : ''}`;
  }

  if (diffHours > 0) {
    const remainingMinutes = diffMinutes % 60;
    if (remainingMinutes > 0) {
      return `resets in ${diffHours}h ${remainingMinutes}m`;
    }
    return `resets in ${diffHours}h`;
  }

  if (diffMinutes > 0) {
    return `resets in ${diffMinutes}m`;
  }

  return 'resets in <1m';
}

export function UsageLimitDisplay({
  label,
  usedPercent,
  resetsAt,
}: UsageLimitDisplayProps) {
  const clampedPercent = Math.min(100, Math.max(0, usedPercent));
  const resetText = formatTimeUntilReset(resetsAt);

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs">
        <span>{label}</span>
        <span className="text-muted-foreground">
          {Math.round(clampedPercent)}%
        </span>
      </div>
      <Progress value={clampedPercent} className="h-1.5" />
      <div className="text-xs text-muted-foreground">{resetText}</div>
    </div>
  );
}
