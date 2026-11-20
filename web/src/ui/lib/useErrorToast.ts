/**
 * Says a failure as a toast: when `error` turns up (a mutation that was refused, a read that failed in the background),
 * the toast tells what could not be done, in `lead`, and what the server said after it. A page calls it with the
 * `error` of its mutation, before any early return, so the failure is not drawn as a banner in the page.
 */
import { useEffect, useRef } from "react";
import { sayError } from "../../api/errorText";
import { useToast } from "../page/Toast";

export function useErrorToast(error: unknown, lead: string): void {
  const toast = useToast();
  const say = useRef({ toast, lead });
  say.current = { toast, lead };
  useEffect(() => {
    // A toast is said once for each failure, so only the error is a dependency.
    if (error) {
      const { toast, lead } = say.current;
      toast({ kind: "error", text: `${lead} ${sayError(error, "Try again.")}` });
    }
  }, [error]);
}
