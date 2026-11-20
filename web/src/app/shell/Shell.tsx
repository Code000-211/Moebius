/**
 * The frame of a signed-in person's page: the textured ground under the whole window, the sidebar lying on it without
 * a surface of its own, and the page as one surface on it — 6px of air above, below and at the right, all four
 * corners round, a shade lighter than the ground and set apart by its edge alone (ADR 0020). The page holds the banner
 * while the server cannot be reached, what the route draws, and its right column (`RightColumn.tsx`) for the page's
 * properties and the Moebius agent, which is part of the same surface behind a soft line. The agent and the package
 * sheet, which pages open from where a person meets plugins and extensions, lie over that column in one place. It owns
 * the command palette and its shortcut, the form of a new issue and `C` (`board/NewIssueHost.tsx`), holds what is in
 * focus for it (`palette/focus.tsx`), and notes the places seen and the sidebar's collapsed state (kept per browser).
 * In the desktop app the ground leaves a strip free at the top for the window's controls, and the rail of servers lies at its
 * far left (`desktop/Rail.tsx`).
 */
import { useQueryClient } from "@tanstack/react-query";
import { Suspense, useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router";
import { desktopBridge } from "../../desktop/bridge";
import { Rail } from "../../desktop/Rail";
import { WINDOW_BAR_CLASS } from "../../desktop/WindowControls";
import { useConnection, useReconnect } from "../../live/LiveProvider";
import { PackageSheet } from "../../packages/PackageSheet";
import { PackageSheetProvider, usePackageSheet } from "../../packages/PackageSheetContext";
import { cn } from "../../ui/lib/cn";
import { isShortcut, modalIsOpen } from "../../ui/lib/shortcuts";
import { PageBoundary } from "../../ui/page/PageBoundary";
import { PropertiesProvider } from "../../ui/page/Properties";
import { SheetSlotProvider } from "../../ui/page/SheetSlot";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { isTyping, NewIssueProvider } from "../board/NewIssueHost";
import { prefetchCard } from "../board/prefetchCard";
import { readFlag, writeFlag } from "../storage";
import { CommandBar } from "./CommandBar";
import { ConnectionBanner } from "./ConnectionBanner";
import { PaletteFocusProvider } from "./palette/focus";
import { noteRecent } from "./palette/model";
import { SHORTCUTS_KEY } from "./palette/shortcuts";
import { RightColumn } from "./RightColumn";
import { Sidebar } from "./Sidebar";

const COLLAPSED_KEY = "moebius.sidebar.collapsed";

export function Shell(props: { unreachable: boolean; onRetry: () => void }) {
  return (
    <SheetSlotProvider>
      <PackageSheetProvider>
        <PropertiesProvider>
          <PaletteFocusProvider>
            <NewIssueProvider>
              <Frame {...props} />
            </NewIssueProvider>
          </PaletteFocusProvider>
        </PropertiesProvider>
      </PackageSheetProvider>
    </SheetSlotProvider>
  );
}

function Frame({ unreachable, onRetry }: { unreachable: boolean; onRetry: () => void }) {
  const packages = usePackageSheet();
  const connection = useConnection();
  const location = useLocation();
  // A card named in the address the page opened at is read now, while the board's code and the snapshot are on their way.
  const queryClient = useQueryClient();
  useEffect(() => {
    const opened = /^\/board\/\d+$/.test(window.location.pathname)
      ? new URLSearchParams(window.location.search).get("card")
      : null;
    const id = Number(opened);
    if (Number.isInteger(id) && id > 0) prefetchCard(queryClient, id);
  }, [queryClient]);
  const reconnect = useReconnect();
  const [collapsed, setCollapsed] = useState(() => readFlag(COLLAPSED_KEY));
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
    <div className={cn("ground flex h-full py-1.5", desktopBridge() && WINDOW_BAR_CLASS)}>
      <Rail bridge={desktopBridge()} />
      <Sidebar
        connection={connection}
        collapsed={collapsed}
        onToggle={() => {
          writeFlag(COLLAPSED_KEY, !collapsed);
          setCollapsed(!collapsed);
        }}
        onSearch={() => setCommandOpen(true)}
      />
      {/* The page: one surface, rounded at all four corners, that clips what scrolls inside it to those corners. The
          sheets lie in it, over its right column, so they share its corners and its height. It clips rather than
          hides, so a sheet that takes the focus while it slides in cannot scroll the whole page sideways. */}
      <div
        data-surface="page"
        className="relative mr-1.5 flex min-w-0 flex-1 overflow-clip rounded-sheet bg-page shadow-edge"
      >
        <div className="flex min-w-0 flex-1 flex-col">
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
              packages.shown && "2xl:pr-[calc(var(--sheet-width)-var(--column-rail-width)+1.5rem)]",
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
        </div>
        <RightColumn />
        <PackageSheet />
      </div>
      <CommandBar open={commandOpen} onOpenChange={setCommandOpen} />
    </div>
  );
}
