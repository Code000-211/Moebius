/**
 * The frame of a signed-in person's page: the textured ground under the whole window, the sidebar lying on it without
 * a surface of its own, and the page as one surface on it — 6px of air above, below and at the right, all four
 * corners round, a shade lighter than the ground and set apart by its edge alone (ADR 0020). The page holds the banner
 * while the server cannot be reached, what the route draws, and its right column for properties, which is part of the
 * same surface behind a soft line. It owns the command palette and its shortcut, holds what is in focus for it (`palette/focus.tsx`), notes the places seen and the sidebar's collapsed state (kept per browser), and holds the package
 * sheet, which pages open from where a person meets plugins and extensions.
 */
import { Suspense, useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";
import { FloatingChat } from "../../chat/FloatingChat";
import { useConnection, useReconnect } from "../../live/LiveProvider";
import { PackageSheet } from "../../packages/PackageSheet";
import { PackageSheetProvider, usePackageSheet } from "../../packages/PackageSheetContext";
import { isShortcut } from "../../ui/lib/shortcuts";
import { PageBoundary } from "../../ui/page/PageBoundary";
import { SheetSlotProvider } from "../../ui/page/SheetSlot";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { readFlag, writeFlag } from "../storage";
import { CommandBar } from "./CommandBar";
import { ConnectionBanner } from "./ConnectionBanner";
import { PaletteFocusProvider } from "./palette/focus";
import { noteRecent } from "./palette/model";
import { Sidebar } from "./Sidebar";

const COLLAPSED_KEY = "moebius.sidebar.collapsed";

export function Shell(props: { unreachable: boolean; onRetry: () => void }) {
  return (
    <SheetSlotProvider>
      <PackageSheetProvider>
        <PaletteFocusProvider>
          <Frame {...props} />
        </PaletteFocusProvider>
      </PackageSheetProvider>
    </SheetSlotProvider>
  );
}

function Frame({ unreachable, onRetry }: { unreachable: boolean; onRetry: () => void }) {
  const packages = usePackageSheet();
  const connection = useConnection();
  const location = useLocation();
  const reconnect = useReconnect();
  const [collapsed, setCollapsed] = useState(() => readFlag(COLLAPSED_KEY));
  const [commandOpen, setCommandOpen] = useState(false);

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
    <div className="ground flex h-full py-1.5">
      <Sidebar
        connection={connection}
        collapsed={collapsed}
        onToggle={() => {
          writeFlag(COLLAPSED_KEY, !collapsed);
          setCollapsed(!collapsed);
        }}
        onSearch={() => setCommandOpen(true)}
      />
      {/* The page: one surface, rounded at all four corners, that clips what scrolls inside it to those corners. */}
      <div
        data-surface="page"
        className="mr-1.5 flex min-w-0 flex-1 overflow-hidden rounded-sheet bg-page shadow-edge"
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
          {/* The package sheet floats over the right of the window; only on a very wide one (2xl) does the page leave
              it its room, so what it was opened from is not covered. Below that the page keeps its width and the sheet
              lies over it: an editor squeezed beside it is worse than one that is covered until the sheet is
              minimised. */}
          <main
            className={`min-h-0 flex-1 overflow-y-auto p-6 ${packages.shown ? "2xl:pr-[33.5rem]" : ""}`}
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
      </div>
      <CommandBar open={commandOpen} onOpenChange={setCommandOpen} />
      <PackageSheet />
      <FloatingChat />
    </div>
  );
}

/**
 * The page's right column, for properties: the same surface as the page, set apart from it by a soft line, with a
 * head at the height of the page's head bar. While no page puts properties in it, it stays the slim column it is
 * folded to.
 */
function RightColumn() {
  return (
    <div
      data-surface="right-column"
      className="flex w-(--column-rail-width) shrink-0 flex-col border-l border-hairline"
    >
      <div data-part="head" className="h-head-bar shrink-0" />
    </div>
  );
}
