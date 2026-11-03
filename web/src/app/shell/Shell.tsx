/**
 * The frame of a signed-in person's page: the sidebar, the banner while the server cannot be reached, and the page.
 * It owns the command bar and its shortcut and the sidebar's collapsed state (kept per browser).
 */
import { Suspense, useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";
import { FloatingChat } from "../../chat/FloatingChat";
import { useConnection, useReconnect } from "../../live/LiveProvider";
import { isShortcut } from "../../ui/lib/shortcuts";
import { PageBoundary } from "../../ui/page/PageBoundary";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { readFlag, writeFlag } from "../storage";
import { CommandBar } from "./CommandBar";
import { ConnectionBanner } from "./ConnectionBanner";
import { Sidebar } from "./Sidebar";

const COLLAPSED_KEY = "moebius.sidebar.collapsed";

export function Shell({ unreachable, onRetry }: { unreachable: boolean; onRetry: () => void }) {
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

  const offline = unreachable || connection === "reconnecting";

  return (
    <div className="flex h-full bg-background">
      <Sidebar
        connection={connection}
        collapsed={collapsed}
        onToggle={() => {
          writeFlag(COLLAPSED_KEY, !collapsed);
          setCollapsed(!collapsed);
        }}
        onSearch={() => setCommandOpen(true)}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        {offline && (
          <ConnectionBanner
            onRetry={() => {
              reconnect();
              onRetry();
            }}
          />
        )}
        <main className="min-h-0 flex-1 overflow-y-auto p-6">
          {/* A page is fetched when it opens, so the shell holds the space it needs while it arrives, and says so
              when it does not: the page is the thing that is missing, not the app around it. */}
          <PageBoundary resetKey={location.pathname}>
            <Suspense fallback={<SkeletonCard lines={4} message="Opening the page" />}>
              <Outlet />
            </Suspense>
          </PageBoundary>
        </main>
      </div>
      <CommandBar open={commandOpen} onOpenChange={setCommandOpen} />
      <FloatingChat />
    </div>
  );
}
