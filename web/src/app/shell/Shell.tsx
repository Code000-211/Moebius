/**
 * The frame of a signed-in person's page: the sidebar, faintly tinted, and the page, white, meeting at one hairline and
 * filling the window to its edges, with no canvas showing round either (`docs/design-web.md`). There is no strip above
 * it and no texture under it. The page holds the banner while the server cannot be reached, what the route draws. The
 * Moebius agent has a panel of its own at the right, opened from the Moebius button in the page header
 * (`AgentButton.tsx`) and by `Ctrl+J`, over the page instead of beside it in a window narrower than 1000px; the package sheet, which pages open from where a person meets plugins and
 * extensions, lies over the right of the same page. In a window narrower than 1000px the sidebar is an overlay opened
 * from the page header (`narrow.ts`, `SidebarToggle.tsx`). It owns the command palette and its shortcut, the form of a
 * new issue and `C` (`board/NewIssueHost.tsx`), the card window for `?card=` on any page (`board/CardWindowHost.tsx`),
 * and holds what is in focus for it (`palette/focus.tsx`). In the desktop app the page carries the window's controls
 * once at its top right (`desktop/WindowControls.tsx`), and the server switch stands in the sidebar's head
 * (`desktop/ServerSwitch.tsx`). The window's round corners are the root's (`tailwind.css`, `data-desktop`).
 */
import { useQueryClient } from "@tanstack/react-query";
import { type CSSProperties, Suspense, useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router";
import { ThinkingSetting } from "../../chat/thinking";
import { desktopBridge } from "../../desktop/bridge";
import { WINDOW_BUTTONS_WIDTH, WindowControls } from "../../desktop/WindowControls";
import { useConnection, useReconnect } from "../../live/LiveProvider";
import { PackageSheet } from "../../packages/PackageSheet";
import { PackageSheetProvider, usePackageSheet } from "../../packages/PackageSheetContext";
import { cn } from "../../ui/lib/cn";
import { isShortcut, modalIsOpen } from "../../ui/lib/shortcuts";
import { PageBoundary } from "../../ui/page/PageBoundary";
import { SheetSlotProvider } from "../../ui/page/SheetSlot";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { CardWindowHost } from "../board/CardWindowHost";
import { isTyping, NewIssueProvider } from "../board/NewIssueHost";
import { prefetchCard } from "../board/prefetchCard";
import { AgentProvider, useAgent } from "./AgentContext";
import { AgentPanel } from "./AgentPanel";
import { CommandBar } from "./CommandBar";
import { ConnectionBanner } from "./ConnectionBanner";
import { useNarrow } from "./narrow";
import { PaletteFocusProvider } from "./palette/focus";
import { noteRecent } from "./palette/model";
import { SHORTCUTS_KEY } from "./palette/shortcuts";
import { Sidebar } from "./Sidebar";
import { SidebarToggleContext } from "./SidebarToggle";

export function Shell(props: { unreachable: boolean; onRetry: () => void }) {
  return (
    <SheetSlotProvider>
      <PackageSheetProvider>
        <PaletteFocusProvider>
          <NewIssueProvider>
            <AgentProvider>
              <ThinkingSetting>
                <Frame {...props} />
              </ThinkingSetting>
            </AgentProvider>
          </NewIssueProvider>
        </PaletteFocusProvider>
      </PackageSheetProvider>
    </SheetSlotProvider>
  );
}

function Frame({ unreachable, onRetry }: { unreachable: boolean; onRetry: () => void }) {
  const packages = usePackageSheet();
  const agent = useAgent();
  // The agent's panel at the panel's right owns the head's right end, with the window's controls.
  const narrow = useNarrow();
  const paneShown = agent.open && !narrow;
  const connection = useConnection();
  const location = useLocation();
  // A card named in the address the page opened at (`?card=`, on any page) is read now, while the board's code and the snapshot are on their way.
  const queryClient = useQueryClient();
  useEffect(() => {
    const id = Number(new URLSearchParams(window.location.search).get("card"));
    if (Number.isInteger(id) && id > 0) prefetchCard(queryClient, id);
  }, [queryClient]);
  const reconnect = useReconnect();
  const [commandOpen, setCommandOpen] = useState(false);
  // In a narrow window the sidebar is an overlay that a page's header opens; choosing a page closes it, and so does
  // growing the window, so it is not found open after a narrow spell.
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const pathname = location.pathname;
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new address is the reason to close
  useEffect(() => setSidebarOpen(false), [pathname, narrow]);

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
    <SidebarToggleContext.Provider value={{ narrow, open: sidebarOpen, setOpen: setSidebarOpen }}>
      <div className="flex h-full bg-sidebar">
        <Sidebar
          narrow={narrow}
          open={sidebarOpen}
          onClose={() => setSidebarOpen(false)}
          onSearch={() => {
            setSidebarOpen(false);
            setCommandOpen(true);
          }}
        />
        {/* The page: the white ground that fills the window to its edges, meeting the sidebar at one hairline (the
          sidebar's right edge). The sheets lie in it, at its right, so they share its height. It clips
          rather than hides, so a sheet that takes the focus while it slides in cannot scroll the whole page sideways.
          The window's controls are in it once; the header reserves their room (`--window-buttons`) unless the
          agent's panel lies at the panel's right, which then leaves it to its own head. */}
        <div data-surface="page" className="relative flex min-w-0 flex-1 overflow-clip bg-panel">
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
            {/* The package sheet lies over the right of the page and a strip of the page beside it. Only on a very wide
              window (2xl) does the page leave it that strip, so what it was opened from is not covered; below that the
              page keeps its width and the sheet lies over it, since an editor squeezed beside it is worse than one
              covered until the sheet is closed. */}
            <main
              className={cn(
                "min-h-0 flex-1 overflow-y-auto p-4",
                packages.shown && "2xl:pr-[calc(var(--sheet-width)+1rem)]",
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
          <AgentPanel />
          <PackageSheet />
          <WindowControls />
        </div>
        <CardWindowHost />
        <CommandBar open={commandOpen} onOpenChange={setCommandOpen} />
      </div>
    </SidebarToggleContext.Provider>
  );
}
