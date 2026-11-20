/**
 * The properties pane inside the page's panel: a column at the panel's right, behind a hairline, that exists only on a
 * page that puts properties in it (`ui/page/Properties.tsx`). A page without any shows no column at all; there is no
 * folded rail, no width to animate and no row of its own for the agent. The pane's head is as high as the page's
 * header, and in the desktop app it leaves the window's controls their place at the panel's top right. While the
 * Moebius agent's panel is open (`AgentPanel.tsx`) the pane is hidden, since the agent has its place; its properties
 * stay drawn, so what a person did in them is still there when the agent is closed.
 */
import { useLayoutEffect } from "react";
import { desktopBridge } from "../../desktop/bridge";
import { WINDOW_BUTTONS_WIDTH } from "../../desktop/WindowControls";
import { useProperties } from "../../ui/page/Properties";
import { useAgent } from "./AgentContext";

export function RightColumn() {
  const { present, setTarget, setShown } = useProperties();
  const agent = useAgent();
  const shown = present && !agent.open;
  // Before paint, so what a page lays beside the pane moves in the same frame the pane does.
  useLayoutEffect(() => setShown(shown), [shown, setShown]);

  return (
    <>
      {/* Always in the tree, since the page's properties are drawn into it; hidden, it takes no room. */}
      <div
        data-surface="right-column"
        hidden={!shown}
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
    </>
  );
}
