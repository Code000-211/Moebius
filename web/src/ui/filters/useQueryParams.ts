/**
 * The address as a page's filter state: the parameters it holds, and a setter that changes some of them in place (no
 * new entry in the history, as the board's filters do). A value of the empty string removes its parameter, so a filter
 * that is not set leaves no trace in the address.
 */
import { useCallback } from "react";
import { useSearchParams } from "react-router";

export function useQueryParams(): [
  URLSearchParams,
  (changes: Readonly<Record<string, string>>) => void,
] {
  const [params, setParams] = useSearchParams();
  const setQuery = useCallback(
    (changes: Readonly<Record<string, string>>) =>
      setParams(
        (current) => {
          const next = new URLSearchParams(current);
          for (const [key, value] of Object.entries(changes)) {
            if (value === "") next.delete(key);
            else next.set(key, value);
          }
          return next;
        },
        { replace: true },
      ),
    [setParams],
  );
  return [params, setQuery];
}
