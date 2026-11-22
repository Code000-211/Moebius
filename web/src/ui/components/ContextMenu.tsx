/**
 * The menu a right click opens at the pointer, with the commands of what is chosen and the key of each. It is a real
 * menu: it takes the focus, the arrow keys move between the items, Enter and a click choose, and Escape, a click
 * elsewhere, a scroll or a resize close it. It keeps every key to itself while it is open, so Escape closes the menu
 * and not the choice behind it. One menu is open at a time: the page holds the point (`useContextMenu`).
 *
 * ```tsx
 * const menu = useContextMenu();
 * <li onContextMenu={(event) => menu.open(event)}>…</li>
 * <ContextMenu point={menu.point} onClose={menu.close} title="3 cards" commands={commands} />
 * ```
 */
import { type MouseEvent, useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { keepKey } from "../lib/shortcuts";
import type { SelectionCommand } from "../page/SelectionBar";

interface Point {
  x: number;
  y: number;
}

/** Where the menu is open, if it is. */
export function useContextMenu() {
  const [point, setPoint] = useState<Point | null>(null);
  const open = useCallback((event: MouseEvent) => {
    event.preventDefault();
    setPoint({ x: event.clientX, y: event.clientY });
  }, []);
  const close = useCallback(() => setPoint(null), []);
  return { point, open, close };
}

export function ContextMenu({
  point,
  title,
  commands,
  onClose,
}: {
  point: Point | null;
  /** What the commands are for, as the menu is named: `3 cards`. */
  title: string;
  commands: readonly SelectionCommand[];
  onClose: () => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!point) return;
    list.current?.querySelector<HTMLElement>("[role=menuitem]:not([aria-disabled=true])")?.focus();
    const away = (event: Event) => {
      if (!list.current?.contains(event.target as Node)) onClose();
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("scroll", onClose, true);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("scroll", onClose, true);
      window.removeEventListener("resize", onClose);
    };
  }, [point, onClose]);
  if (!point) return null;
  const move = (step: number) => {
    const items = [
      ...(list.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? []),
    ].filter((item) => item.getAttribute("aria-disabled") !== "true");
    const at = items.indexOf(document.activeElement as HTMLElement);
    items[(at + step + items.length) % items.length]?.focus();
  };
  return createPortal(
    <div
      ref={list}
      role="menu"
      aria-label={title}
      style={{
        left: Math.max(8, Math.min(point.x, window.innerWidth - 248)),
        top: Math.max(8, Math.min(point.y, window.innerHeight - 40 * (commands.length + 1))),
      }}
      onKeyDown={(event) => {
        keepKey(event);
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        } else if (event.key === "ArrowDown") {
          event.preventDefault();
          move(1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          move(-1);
        }
      }}
      onContextMenu={(event) => event.preventDefault()}
      className="glass fixed z-menu w-60 rounded-card p-1 text-text shadow-raised"
    >
      <p className="m-0 px-2 py-1 text-xs text-secondary">{title}</p>
      {commands.map((command) => (
        <div
          key={command.id}
          role="menuitem"
          tabIndex={-1}
          aria-disabled={command.disabled || undefined}
          aria-keyshortcuts={command.key}
          onClick={() => {
            if (command.disabled) return;
            onClose();
            command.run();
          }}
          onKeyDown={(event) => {
            if ((event.key === "Enter" || event.key === " ") && !command.disabled) {
              event.preventDefault();
              onClose();
              command.run();
            }
          }}
          className="flex min-h-8 cursor-pointer select-none items-center gap-2 rounded-control px-2 text-base outline-none hover:bg-selected focus:bg-selected aria-disabled:pointer-events-none aria-disabled:opacity-45"
        >
          <span className="flex-1">{command.label}</span>
        </div>
      ))}
    </div>,
    document.body,
  );
}
