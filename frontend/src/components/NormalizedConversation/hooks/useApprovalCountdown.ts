import { useEffect, useMemo, useState } from 'react';

/**
 * Hook for managing countdown timer for approval/question timeouts.
 * @param requestedAt - Timestamp when the request was created
 * @param timeoutAt - Timestamp when the request will timeout (null = no timeout)
 * @param paused - Whether to pause the countdown (e.g., after user responds)
 * @returns timeLeft (seconds remaining, -1 if no timeout), percent (0-100, 100 if no timeout), hasTimeout
 */
export function useApprovalCountdown(
  requestedAt: string | number | Date,
  timeoutAt: string | number | Date | null | undefined,
  paused: boolean
) {
  const hasTimeout = timeoutAt != null;

  const totalSeconds = useMemo(() => {
    if (!hasTimeout) return Infinity;
    const total = Math.floor(
      (new Date(timeoutAt).getTime() - new Date(requestedAt).getTime()) / 1000
    );
    return Math.max(1, total);
  }, [requestedAt, timeoutAt, hasTimeout]);

  const [timeLeft, setTimeLeft] = useState<number>(() => {
    if (!hasTimeout) return -1;
    const remaining = new Date(timeoutAt).getTime() - Date.now();
    return Math.max(0, Math.floor(remaining / 1000));
  });

  useEffect(() => {
    if (paused || !hasTimeout) return;
    const id = window.setInterval(() => {
      const remaining = new Date(timeoutAt).getTime() - Date.now();
      const next = Math.max(0, Math.floor(remaining / 1000));
      setTimeLeft(next);
      if (next <= 0) window.clearInterval(id);
    }, 1000);

    return () => window.clearInterval(id);
  }, [timeoutAt, paused, hasTimeout]);

  const percent = useMemo(() => {
    if (!hasTimeout) return 100;
    return Math.max(0, Math.min(100, Math.round((timeLeft / totalSeconds) * 100)));
  }, [timeLeft, totalSeconds, hasTimeout]);

  return { timeLeft, percent, hasTimeout };
}
