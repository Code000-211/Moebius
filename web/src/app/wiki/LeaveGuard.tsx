/**
 * Asks before a person leaves the wiki editor with text that is not saved, whether by an app route (another page of
 * the tree, Back, Cancel) or by reloading or closing the tab. The tab shows the browser's own message; nothing of
 * ours can be put in front of it. `dirty` is read when the person tries to leave, so an editor that has just saved
 * can say there is nothing left to lose before it navigates.
 */

import { useEffect } from "react";
import { useBlocker } from "react-router";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";

export function LeaveGuard({ dirty }: { dirty: () => boolean }) {
  // The wiki keeps what is open in the query (`?page=`), so a change of it is a way out as much as another path is.
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      dirty() &&
      currentLocation.pathname + currentLocation.search !==
        nextLocation.pathname + nextLocation.search,
  );

  useEffect(() => {
    function ask(event: BeforeUnloadEvent) {
      if (!dirty()) return;
      event.preventDefault();
      event.returnValue = true;
    }
    window.addEventListener("beforeunload", ask);
    return () => window.removeEventListener("beforeunload", ask);
  }, [dirty]);

  return (
    <Dialog open={blocker.state === "blocked"} onOpenChange={(next) => !next && blocker.reset?.()}>
      <DialogContent className="gap-4 p-6">
        <DialogHeader>
          <DialogTitle>Leave with unsaved text?</DialogTitle>
          <DialogDescription>
            What you wrote in the editor is not saved yet. Leaving now loses it.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="secondary" onClick={() => blocker.reset?.()}>
            Keep editing
          </Button>
          <Button variant="danger" onClick={() => blocker.proceed?.()}>
            Leave without saving
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
