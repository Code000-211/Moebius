/**
 * The properties pane inside the page's panel: a column at the panel's right, behind a hairline, that exists only on a
 * page that puts properties in it (`ui/page/Properties.tsx`). A page without any shows no column at all; there is no
 * folded rail, no width to animate and no row of its own for the agent. The pane's head is as high as the page's
 * header, and in the desktop app it leaves the window's controls their place at the panel's top right. The properties
 * stay drawn while a sheet lies over the pane, so what a person did in them is still there when they come back.
 *
 * `Ctrl+J` opens the Moebius agent (`chat/AgentChat.tsx`) over the pane, and closes it again; the agent lies in the
 * same panel, over the pane's place.
 */
import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState } from "react";
import { desktopBridge } from "../../desktop/bridge";
import { WINDOW_BUTTONS_WIDTH } from "../../desktop/WindowControls";
import { isShortcut, modalIsOpen } from "../../ui/lib/shortcuts";
import { useProperties } from "../../ui/page/Properties";
import { useSheetSlot } from "../../ui/page/SheetSlot";

// The chat sheet, with the Markdown and the thread it draws, is loaded when the agent is first opened, not with the page.
const AgentChat = lazy(() =>
  import("../../chat/AgentChat").then((all) => ({ default: all.AgentChat })),
);

export function RightColumn() {
  const { present, setTarget, setShown } = useProperties();
  // Before paint, so what a page lays beside the pane moves in the same frame the pane does.
  useLayoutEffect(() => setShown(present), [present, setShown]);
  const slot = useSheetSlot();
  // Once the chat was shown it stays mounted, so closing the sheet keeps the conversation where it was.
  const [asked, setAsked] = useState(false);
  if (slot.active === "chat" && !asked) setAsked(true);
  const opener = useRef<HTMLButtonElement>(null);

  // `Ctrl+J` opens the agent and closes it again, from anywhere but a modal dialog, which keeps the keyboard to itself.
  const { active, show, hide } = slot;
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (!isShortcut(event, "j") || modalIsOpen()) return;
      event.preventDefault();
      if (active === "chat") hide("chat");
      else show("chat");
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, show, hide]);

  return (
    <>
      {/* Always in the tree, since the page's properties are drawn into it; hidden, it takes no room. */}
      <div
        data-surface="right-column"
        hidden={!present}
        className="flex w-(--column-width) shrink-0 flex-col border-l border-hairline"
      >
        <div
          data-part="head"
          className="flex h-12 shrink-0 items-center border-b border-hairline pl-4"
          style={{ paddingRight: desktopBridge() ? WINDOW_BUTTONS_WIDTH : "1rem" }}
        >
          <h2 className="m-0 text-sm font-semibold text-text">Properties</h2>
        </div>
        <div ref={setTarget} data-part="properties" className="min-h-0 flex-1 overflow-y-auto" />
      </div>
      {asked && (
        <Suspense fallback={null}>
          <AgentChat opener={opener} onProperties={present ? () => hide("chat") : undefined} />
        </Suspense>
      )}
    </>
  );
}
