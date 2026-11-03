/**
 * How the sheet makes a request: one at a time, so a second press of a button is not a second install, with the answer
 * handed to `onReport` as the words the sheet shows — or the reason it was refused. Whatever packages the sheet and the
 * Plugins page read is read again afterwards, since the live events only arrive when the socket is up.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import { keys } from "../api/keys";
import { refusal } from "./model";
import { type Report, refused } from "./Report";

export interface Actions {
  /** A request is on its way: the buttons wait. */
  busy: boolean;
  /** Makes `work`, tells its answer with `tell`, or says why it was refused (`fallback` when the server gave no reason). */
  run: <T>(work: () => Promise<T>, tell: (answer: T) => Report, fallback: string) => Promise<void>;
}

export function useActions(onReport: (report: Report | null) => void): Actions {
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const going = useRef(false);
  const run = useCallback(
    async <T>(work: () => Promise<T>, tell: (answer: T) => Report, fallback: string) => {
      if (going.current) return;
      going.current = true;
      setBusy(true);
      onReport(null);
      try {
        onReport(tell(await work()));
      } catch (error) {
        onReport(refused(refusal(error, fallback)));
      } finally {
        going.current = false;
        setBusy(false);
        // The lists, not what a package opened in the sheet read: a Pi package is fetched from npm to be read.
        for (const queryKey of [
          keys.packagesSearch,
          keys.packagesInstalled,
          keys.packagesInfo,
          keys.piSearchAll,
          keys.piInstalled,
          keys.plugins,
        ]) {
          void queryClient.invalidateQueries({ queryKey });
        }
        void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      }
    },
    [onReport, queryClient],
  );
  return { busy, run };
}
