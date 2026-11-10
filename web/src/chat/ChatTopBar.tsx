/**
 * The bar on top of the chat: the title of the chat on the left and its actions on the right. The list of chats it
 * used to switch to lives in the shell's sidebar now, and a new chat starts from the sidebar's own button, so the bar
 * carries nothing of the list. It is a row of controls over the canvas rather than a panel of its own, and every
 * control in it is the shared `Button`, at the size the header of every other page uses, so the row of controls under
 * a title is the same row wherever it is met.
 */
import type { ReactNode } from "react";

/** The title row of the chat page: the title on the left, the actions on the right. */
export function ChatTopBar({ title, actions }: { title?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2">
      <div className="flex min-w-0 flex-1 items-center gap-2">{title}</div>
      {actions}
    </div>
  );
}
