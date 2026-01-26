import { Progress } from '@/components/ui/progress';

export interface WeekProgressTrackerProps {
  resetsAt: string;
}

const WEEK_MS = 7 * 24 * 60 * 60 * 1000;

function formatTimeUntilRenewal(resetsAt: string): string {
  const resetTime = new Date(resetsAt).getTime();
  const now = Date.now();
  const diffMs = resetTime - now;

  if (diffMs <= 0) {
    return 'Renewing...';
  }

  const diffMinutes = Math.floor(diffMs / (1000 * 60));
  const diffHours = Math.floor(diffMinutes / 60);
  const diffDays = Math.floor(diffHours / 24);
  const remainingHours = diffHours % 24;

  if (diffDays > 0) {
    if (remainingHours > 0) {
      return `Renews in ${diffDays}d ${remainingHours}h`;
    }
    return `Renews in ${diffDays}d`;
  }

  if (diffHours > 0) {
    const remainingMinutes = diffMinutes % 60;
    if (remainingMinutes > 0) {
      return `Renews in ${diffHours}h ${remainingMinutes}m`;
    }
    return `Renews in ${diffHours}h`;
  }

  if (diffMinutes > 0) {
    return `Renews in ${diffMinutes}m`;
  }

  return 'Renews in <1m';
}

function calculateWeekProgress(resetsAt: string): number {
  const resetTime = new Date(resetsAt).getTime();
  const now = Date.now();
  const timeRemaining = resetTime - now;

  if (timeRemaining <= 0) {
    return 100;
  }

  const timeElapsed = WEEK_MS - timeRemaining;
  const progress = (timeElapsed / WEEK_MS) * 100;

  return Math.max(0, Math.min(100, progress));
}

export function WeekProgressTracker({ resetsAt }: WeekProgressTrackerProps) {
  const progress = calculateWeekProgress(resetsAt);
  const renewalText = formatTimeUntilRenewal(resetsAt);

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs">
        <span>Weekly cycle</span>
        <span className="text-muted-foreground">{Math.round(progress)}%</span>
      </div>
      <Progress value={progress} className="h-1.5" />
      <div className="text-xs text-muted-foreground">{renewalText}</div>
    </div>
  );
}
