import { useCallback, useEffect, useRef, useState } from "react";

/**
 * `value`, as it was `delayMs` ago: it follows only once the input has stopped changing. The
 * second item takes a value at once and drops the one still waiting, for a value set by the
 * code rather than typed (a search cleared after an add): a late typed value then cannot land
 * after it.
 */
export function useDebounced<T>(value: T, delayMs: number): [T, (now: T) => void] {
  const [debounced, setDebounced] = useState(value);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => {
    timer.current = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer.current);
  }, [value, delayMs]);
  const settle = useCallback((now: T) => {
    clearTimeout(timer.current);
    setDebounced(now);
  }, []);
  return [debounced, settle];
}
