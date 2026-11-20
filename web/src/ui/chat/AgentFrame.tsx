/**
 * The right panel the Moebius agent lies in: a solid column of the page's panel, beside the page and behind a hairline,
 * with the agent's name and one control, the one that closes it. In the desktop app the head leaves the window's
 * controls their place at the panel's top right. The conversation and the reply field are its children.
 */

import { XIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { desktopBridge } from "../../desktop/bridge";
import { WINDOW_BUTTONS_WIDTH } from "../../desktop/WindowControls";
import { Button } from "../components/Button";

export function AgentFrame({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  return (
    <aside
      aria-label="Moebius agent"
      data-surface="agent-panel"
      className="flex w-(--agent-width) max-w-[45%] shrink-0 animate-in flex-col border-l border-hairline bg-page fade-in-0 duration-(--motion-fast) ease-(--motion-ease)"
    >
      <header
        className="flex h-12 shrink-0 items-center gap-2 border-b border-hairline pl-4"
        style={{ paddingRight: desktopBridge() ? WINDOW_BUTTONS_WIDTH : "0.75rem" }}
      >
        <span
          aria-hidden="true"
          className="flex size-6 items-center justify-center rounded-md bg-text text-xs font-semibold text-on-ink"
        >
          M
        </span>
        <h2 className="m-0 min-w-0 flex-1 text-sm font-semibold text-text">Moebius</h2>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label="Close the Moebius agent"
          title="Close the Moebius agent"
          onClick={onClose}
        >
          <XIcon aria-hidden="true" className="size-4" weight="bold" />
        </Button>
      </header>
      <div className="flex min-h-0 flex-1 flex-col gap-3 p-3">{children}</div>
    </aside>
  );
}
