/**
 * The page's right column (ADR 0020): the same surface as the page, set apart from it by a soft line, with a head at
 * the height of the page's head bar. It holds the properties a page puts in it (`ui/page/Properties.tsx`) and, as its
 * last row, "Ask Moebius" (`Ctrl+J`), which opens the Moebius agent over it (`chat/AgentChat.tsx`). Folded, and on a
 * page without properties, it stays a slim column with the button that unfolds it and the agent's button; whether a
 * person folded it is kept for this browser, so it stays folded from page to page. The properties stay drawn while
 * the column is folded or a sheet lies over it, so what a person did in them is still there when they come back.
 */
import { ChatCircleDotsIcon, SidebarSimpleIcon } from "@phosphor-icons/react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { AgentChat } from "../../chat/AgentChat";
import { cn } from "../../ui/lib/cn";
import { getModifierKey } from "../../ui/lib/platform";
import { isShortcut, modalIsOpen } from "../../ui/lib/shortcuts";
import { useProperties } from "../../ui/page/Properties";
import { useSheetSlot } from "../../ui/page/SheetSlot";
import { readFlag, writeFlag } from "../storage";

const FOLDED_KEY = "moebius.properties.folded";

const ICON_BUTTON =
  "inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-control text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:bg-transparent";

export function RightColumn() {
  const { present, setTarget, setShown } = useProperties();
  const [folded, setFolded] = useState(() => readFlag(FOLDED_KEY));
  const open = present && !folded;
  // Before paint, so what a page lays beside the column moves in the same frame the column does.
  useLayoutEffect(() => setShown(open), [open, setShown]);
  const slot = useSheetSlot();
  const ask = useRef<HTMLButtonElement>(null);
  const modifier = getModifierKey();

  const fold = (next: boolean) => {
    writeFlag(FOLDED_KEY, next);
    setFolded(next);
  };

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
      <div
        data-surface="right-column"
        data-folded={open ? undefined : ""}
        className={cn(
          "flex shrink-0 flex-col border-l border-hairline transition-[width] duration-(--motion-slow) ease-(--motion-ease)",
          open ? "w-(--column-width)" : "w-(--column-rail-width)",
        )}
      >
        <div
          data-part="head"
          className={cn(
            "flex h-head-bar shrink-0 items-center",
            open ? "gap-2 border-b border-hairline pr-1.5 pl-4" : "justify-center",
          )}
        >
          {open ? (
            <>
              <h2 className="m-0 flex-1 text-sm font-semibold text-text">Properties</h2>
              <button
                type="button"
                className={ICON_BUTTON}
                aria-label="Fold the properties"
                title="Fold the properties"
                onClick={() => fold(true)}
              >
                <SidebarSimpleIcon
                  aria-hidden="true"
                  className="size-4 -scale-x-100"
                  weight="bold"
                />
              </button>
            </>
          ) : (
            <button
              type="button"
              className={ICON_BUTTON}
              aria-label="Show the properties"
              title={present ? "Show the properties" : "This page has no properties"}
              disabled={!present}
              onClick={() => fold(false)}
            >
              <SidebarSimpleIcon aria-hidden="true" className="size-4 -scale-x-100" weight="bold" />
            </button>
          )}
        </div>
        {/* Where a page's properties are drawn. Folded, it is hidden rather than emptied, so they keep their state. */}
        <div
          ref={setTarget}
          data-part="properties"
          hidden={!open}
          className="min-h-0 flex-1 overflow-y-auto"
        />
        <div className={cn("mt-auto shrink-0 p-1.5", !open && "flex justify-center")}>
          <button
            ref={ask}
            type="button"
            aria-label="Ask Moebius"
            aria-keyshortcuts="Control+J"
            title={`Ask Moebius · ${modifier} J`}
            aria-expanded={slot.active === "chat"}
            onClick={() => show("chat")}
            className={cn(ICON_BUTTON, open && "w-full justify-start gap-2 px-2.5 text-sm")}
          >
            <ChatCircleDotsIcon aria-hidden="true" className="size-4 shrink-0" weight="bold" />
            {open && (
              <>
                <span className="flex-1 text-left">Ask Moebius</span>
                <kbd className="font-mono text-xs text-secondary">{modifier} J</kbd>
              </>
            )}
          </button>
        </div>
      </div>
      <AgentChat opener={ask} onProperties={present ? () => fold(false) : undefined} />
    </>
  );
}
