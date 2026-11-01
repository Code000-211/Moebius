/**
 * A shell in the clone of a repository, in a sheet over the board, with the board still readable behind it.
 *
 * The sheet is not where the shell lives. React takes a sheet's content away when it closes, and a terminal's box
 * going away closes its socket, which ends the shell on the server — so a sheet that owned the shell would throw away
 * the working directory, the scrollback and whatever was running in it every time a person closed it to look at a
 * card. The pane is therefore a box of this component's own, outside React's hands, which the sheet borrows while it
 * is open and hands back when it is not; the terminal is rendered into it through a portal, so React still draws
 * everything a person sees inside it. The shell starts the first time the sheet is opened, not when the board is
 * read, and ends when the page is left.
 */
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Terminal } from "../../terminal/Terminal";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "../../ui/components/Dialog";

export function RepositoryTerminal({
  repository,
  open,
  onOpenChange,
}: {
  repository: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  // The box xterm draws into. React never creates or removes it, so it can be moved between boxes without the
  // shell in it noticing. It is the width of whatever shows it, and takes the height it is given.
  const [pane] = useState(() => {
    const box = document.createElement("div");
    box.className = "flex min-h-0 flex-1 flex-col";
    return box;
  });
  // Where the pane waits while no sheet is showing it: a box in the page that draws nothing.
  const dock = useRef<HTMLDivElement | null>(null);
  const [started, setStarted] = useState(open);

  // The shell starts when the sheet is first opened, not when the board is opened, and then lasts as long as the page.
  useLayoutEffect(() => {
    if (open) setStarted(true);
  }, [open]);

  useLayoutEffect(() => {
    const held = dock.current;
    if (held && pane.parentElement !== held) held.appendChild(pane);
  }, [pane]);

  // The sheet's box takes the pane as it appears and gives it back as it goes away. React calls this before it
  // removes the box, so the pane is never carried off with the closed sheet: the socket stays open, and the shell is
  // still where the person left it the next time the sheet opens.
  const takeBox = useCallback(
    (box: HTMLDivElement | null) => {
      const held = dock.current;
      if (box) {
        if (pane.parentElement !== box) box.appendChild(pane);
      } else if (held && pane.parentElement !== held) held.appendChild(pane);
    },
    [pane],
  );

  return (
    <>
      <div ref={dock} hidden />
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent
          data-testid="board-terminal"
          aria-describedby={undefined}
          className="h-[min(34rem,85vh)] max-w-5xl"
        >
          <div className="flex min-h-0 flex-1 flex-col gap-4 p-6">
            <DialogHeader>
              <DialogTitle>Terminal</DialogTitle>
            </DialogHeader>
            <div ref={takeBox} className="flex min-h-0 flex-1 flex-col" />
          </div>
        </DialogContent>
      </Dialog>
      {started &&
        createPortal(
          <Terminal
            path={`/v1/repositories/${repository}/terminal`}
            autoFocus
            focused={open}
            className="min-h-0 flex-1"
          />,
          pane,
        )}
    </>
  );
}
