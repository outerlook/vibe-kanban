import { useEffect, useState } from 'react';
import { Loader2, CheckCircle, AlertCircle } from 'lucide-react';
import { cn } from '@/lib/utils';

export type ConnectionStatusBannerProps = Readonly<{
  isConnected: boolean;
  error: string | null;
  className?: string;
}>;

export function ConnectionStatusBanner({
  isConnected,
  error,
  className,
}: ConnectionStatusBannerProps) {
  const [showReconnected, setShowReconnected] = useState(false);
  const [wasDisconnected, setWasDisconnected] = useState(false);

  useEffect(() => {
    if (!isConnected) {
      setWasDisconnected(true);
    } else if (wasDisconnected) {
      setShowReconnected(true);
      const timer = setTimeout(() => {
        setShowReconnected(false);
        setWasDisconnected(false);
      }, 3000);
      return () => clearTimeout(timer);
    }
  }, [isConnected, wasDisconnected]);

  // Error state
  if (error) {
    return (
      <div
        className={cn(
          'flex items-center gap-2 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-destructive',
          className
        )}
        role="status"
        aria-live="polite"
      >
        <AlertCircle className="h-4 w-4 shrink-0" aria-hidden />
        <span className="text-sm">{error}</span>
      </div>
    );
  }

  // Reconnecting state
  if (!isConnected) {
    return (
      <div
        className={cn(
          'flex items-center gap-2 rounded-md border border-warning/40 bg-warning/10 p-3 text-warning-foreground dark:text-warning',
          className
        )}
        role="status"
        aria-live="polite"
      >
        <Loader2 className="h-4 w-4 shrink-0 animate-spin" aria-hidden />
        <span className="text-sm">Reconnecting...</span>
      </div>
    );
  }

  // Brief reconnected confirmation
  if (showReconnected) {
    return (
      <div
        className={cn(
          'flex items-center gap-2 rounded-md border border-green-500/40 bg-green-500/10 p-3 text-green-700 dark:text-green-400',
          className
        )}
        role="status"
        aria-live="polite"
      >
        <CheckCircle className="h-4 w-4 shrink-0" aria-hidden />
        <span className="text-sm">Reconnected</span>
      </div>
    );
  }

  // Connected - don't render
  return null;
}
