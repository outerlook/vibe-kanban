import { useRef, useEffect, useCallback } from 'react';

export interface UseLongPressOptions {
  delay?: number; // default 500ms
  threshold?: number; // movement threshold in px, default 10
}

export interface PointerEventHandlers {
  onPointerDown: (e: React.PointerEvent) => void;
  onPointerUp: (e: React.PointerEvent) => void;
  onPointerMove: (e: React.PointerEvent) => void;
  onPointerCancel: (e: React.PointerEvent) => void;
}

/**
 * Hook to detect long-press gestures via pointer events.
 * Works for both touch and mouse input.
 */
export function useLongPress(
  callback: () => void,
  options?: UseLongPressOptions
): PointerEventHandlers {
  const { delay = 500, threshold = 10 } = options ?? {};

  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const callbackRef = useRef(callback);
  const startPosRef = useRef<{ x: number; y: number } | null>(null);

  // Keep callback ref up to date
  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  const cancel = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
    startPosRef.current = null;
  }, []);

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      startPosRef.current = { x: e.clientX, y: e.clientY };
      timeoutRef.current = setTimeout(() => {
        callbackRef.current();
        timeoutRef.current = null;
      }, delay);
    },
    [delay]
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!startPosRef.current || !timeoutRef.current) return;

      const dx = e.clientX - startPosRef.current.x;
      const dy = e.clientY - startPosRef.current.y;
      const distance = Math.sqrt(dx * dx + dy * dy);

      if (distance > threshold) {
        cancel();
      }
    },
    [threshold, cancel]
  );

  const onPointerUp = useCallback(() => {
    cancel();
  }, [cancel]);

  const onPointerCancel = useCallback(() => {
    cancel();
  }, [cancel]);

  return {
    onPointerDown,
    onPointerUp,
    onPointerMove,
    onPointerCancel,
  };
}
