/**
 * The frame of a signed-in person's page: the canvas under the whole window, flat and faintly tinted, with the sidebar
 * lying on it without a surface of its own, and the page as one panel floating on it — 6px of air above, below and at
 * the right, all four corners round, a hairline for its edge and no shadow (`docs/design-web.md`). There is no strip
 * above it and no texture under it. The panel holds the banner while the server cannot be reached, what the route
 * draws, and, only on a page that supplies properties, the pane for them (`RightColumn.tsx`). The Moebius agent has a
 * panel of its own at the right, opened from the dock, the quiet field at the bottom of the page (`AgentDock.tsx`), and
 * by `Ctrl+J`; the package sheet, which pages open from where a person meets plugins and extensions, lies over the
 * properties pane's place in the same panel. It
 * owns the command palette and its shortcut, the form of a new issue and `C` (`board/NewIssueHost.tsx`), the card window for `?card=` on any page (`board/CardWindowHost.tsx`), holds what is
 * in focus for it (`palette/focus.tsx`), and notes the places seen and the sidebar's collapsed state (kept per
 * browser). In the desktop app the panel carries the window's controls once at its top right
 * (`desktop/WindowControls.tsx`), and the rail of servers lies at the window's far left (`desktop/Rail.tsx`).
 */
import { useQueryClient } from "@tanstack/react-query";
import { type CSSProperties, Suspense, useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router";
import { ThinkingSetting } from "../../chat/thinking";
import { desktopBridge } from "../../desktop/bridge";
import { Rail } from "../../desktop/Rail";
import { WINDOW_BUTTONS_WIDTH, WindowControls } from "../../desktop/WindowControls";
import { useConnection, useReconnect } from "../../live/LiveProvider";
import { PackageSheet } from "../../packages/PackageSheet";
import { PackageSheetProvider, usePackageSheet } from "../../packages/PackageSheetContext";
import { cn } from "../../ui/lib/cn";
import { isShortcut, modalIsOpen } from "../../ui/lib/shortcuts";
import { PageBoundary } from "../../ui/page/PageBoundary";
import { PropertiesProvider, useProperties } from "../../ui/page/Properties";
import { SheetSlotProvider } from "../../ui/page/SheetSlot";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { CardWindowHost } from "../board/CardWindowHost";
import { isTyping, NewIssueProvider } from "../board/NewIssueHost";
import { prefetchCard } from "../board/prefetchCard";
import { inSettingsArea } from "../settings/settingsEntries";
import { readFlag, writeFlag } from "../storage";
import { AgentProvider, useAgent } from "./AgentContext";
import { AgentDock } from "./AgentDock";
import { AgentPanel } from "./AgentPanel";
import { CommandBar } from "./CommandBar";
import { ConnectionBanner } from "./ConnectionBanner";
import { PaletteFocusProvider } from "./palette/focus";
import { noteRecent } from "./palette/model";
import { SHORTCUTS_KEY } from "./palette/shortcuts";
import { RightColumn } from "./RightColumn";
import { Sidebar } from "./Sidebar";

const COLLAPSED_KEY = "moebius.sidebar.collapsed";

/** The window width under which the sidebar folds to its rail by itself, as the pages need the width more. */
const NARROW_BELOW = 1024;

/** Whether the window is narrower than `NARROW_BELOW`, following it as it is resized. */
function useNarrow(): boolean {
  const query = `(max-width: ${NARROW_BELOW - 1}px)`;
  const [narrow, setNarrow] = useState(
    () => typeof matchMedia === "function" && matchMedia(query).matches,
  );
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const media = matchMedia(query);
    const change = () => setNarrow(media.matches);
    change();
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, [query]);
  return narrow;
}

export function Shell(props: { unreachable: boolean; onRetry: () => void }) {
  return (
    <SheetSlotProvider>
      <PackageSheetProvider>
        <PropertiesProvider>
          <PaletteFocusProvider>
            <NewIssueProvider>
              <AgentProvider>
                <ThinkingSetting>
                  <Frame {...props} />
                </ThinkingSetting>
              </AgentProvider>
            </NewIssueProvider>
          </PaletteFocusProvider>
        </PropertiesProvider>
      </PackageSheetProvider>
    </SheetSlotProvider>
  );
}

function Frame({ unreachable, onRetry }: { unreachable: boolean; onRetry: () => void }) {
  const packages = usePackageSheet();
  const { present: hasProperties } = useProperties();
  const agent = useAgent();
  // The pane at the panel's right, the properties or the agent, owns the head's right end, with the window's controls.
  const paneShown = hasProperties || agent.open;
  const connection = useConnection();
  const location = useLocation();
  // A card named in the address the page opened at (`?card=`, on any page) is read now, while the board's code and the snapshot are on their way.
  const queryClient = useQueryClient();
  useEffect(() => {
    const id = Number(new URLSearchParams(window.location.search).get("card"));
    if (Number.isInteger(id) && id > 0) prefetchCard(queryClient, id);
  }, [queryClient]);
  const reconnect = useReconnect();
  const [collapsed, setCollapsed] = useState(() => readFlag(COLLAPSED_KEY));
  const narrow = useNarrow();
  // Below `NARROW_BELOW` the sidebar is the rail unless a person opened it for now; the saved choice is left as it was.
  const [openedNarrow, setOpenedNarrow] = useState(false);
  const folded = narrow ? !openedNarrow : collapsed;
  const [commandOpen, setCommandOpen] = useState(false);

  const navigate = useNavigate();
  useEffect(() => {
    // `?` opens the page that lists the shortcuts, where a key belongs to the page: not in a field, not in a dialog.
    function onQuestion(event: KeyboardEvent) {
      if (event.key !== SHORTCUTS_KEY || event.defaultPrevented) return;
      if (event.ctrlKey || event.metaKey || event.altKey || event.repeat) return;
      if (isTyping(event.target) || modalIsOpen()) return;
      event.preventDefault();
      void navigate("/shortcuts");
    }
    window.addEventListener("keydown", onQuestion);
    return () => window.removeEventListener("keydown", onQuestion);
  }, [navigate]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      // The bar is the only thing the shortcut opens, so it closes it too, which is why the bar's own scrim is not a
      // reason to ignore the key. `⌘⇧K` and a key a control already took belong to something else.
      if (!isShortcut(event, "k")) return;
      event.preventDefault();
      setCommandOpen((open) => !open);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // The places the palette offers again; the page a person left is the one worth going back to.
  useEffect(() => {
    noteRecent(location.pathname);
  }, [location.pathname]);

  const offline = unreachable || connection === "reconnecting";

  return (
    <div className="flex h-full bg-ground py-1.5">
      <Rail bridge={desktopBridge()} />
      <Sidebar
        connection={connection}
        collapsed={folded}
        onToggle={() => {
          if (narrow) {
            setOpenedNarrow(!openedNarrow);
            return;
          }
          writeFlag(COLLAPSED_KEY, !collapsed);
          setCollapsed(!collapsed);
        }}
        onSearch={() => setCommandOpen(true)}
      />
      {/* The page: one panel, rounded at all four corners, that clips what scrolls inside it to those corners. The
          sheets lie in it, over the place of its properties pane, so they share its corners and its height. It clips
          rather than hides, so a sheet that takes the focus while it slides in cannot scroll the whole page sideways.
          The window's controls are in it once; the header reserves their room (`--window-buttons`) unless the
          properties pane lies at the panel's right, which then leaves it to its own head. */}
      <div
        data-surface="page"
        className="relative mr-1.5 flex min-w-0 flex-1 overflow-clip rounded-sheet border border-hairline bg-page"
      >
        <div
          className="flex min-w-0 flex-1 flex-col"
          style={
            desktopBridge() && !paneShown
              ? ({ "--window-buttons": WINDOW_BUTTONS_WIDTH } as CSSProperties)
              : undefined
          }
        >
          {offline && (
            <ConnectionBanner
              onRetry={() => {
                reconnect();
                onRetry();
              }}
            />
          )}
          {/* The package sheet lies over the right column and a strip of the page beside it. Only on a very wide
              window (2xl) does the page leave it that strip, so what it was opened from is not covered; below that the
              page keeps its width and the sheet lies over it, since an editor squeezed beside it is worse than one
              covered until the sheet is closed. */}
          <main
            className={cn(
              "min-h-0 flex-1 overflow-y-auto p-6",
              packages.shown && "2xl:pr-[calc(var(--sheet-width)+1.5rem)]",
            )}
          >
            {/* A page is fetched when it opens, so the shell holds the space it needs while it arrives, and says so
                when it does not: the page is the thing that is missing, not the app around it. */}
            <PageBoundary resetKey={location.pathname}>
              <Suspense fallback={<SkeletonCard lines={4} message="Opening the page" />}>
                <Outlet />
              </Suspense>
            </PageBoundary>
          </main>
          {/* The quiet field for the agent; not while the agent's panel is open, and not on Settings, which has none. */}
          {!agent.open && !inSettingsArea(location.pathname) && <AgentDock />}
        </div>
        <AgentPanel />
        <RightColumn />
        <PackageSheet />
        <WindowControls />
      </div>
      <CardWindowHost />
      <CommandBar open={commandOpen} onOpenChange={setCommandOpen} />
    </div>
  );
}
