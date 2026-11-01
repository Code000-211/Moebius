/**
 * The frame of a signed-in person's page: the sidebar, the banner while the server cannot be reached, and the page.
 * It owns the command bar and its shortcut and the sidebar's collapsed state (kept per browser).
 */
import { useEffect, useState } from "react";
import { Outlet } from "react-router";
import { FloatingChat } from "../../chat/FloatingChat";
import { useConnection, useReconnect } from "../../live/LiveProvider";
import { readFlag, writeFlag } from "../storage";
import { CommandBar } from "./CommandBar";
import { ConnectionBanner } from "./ConnectionBanner";
import { Sidebar } from "./Sidebar";

const COLLAPSED_KEY = "moebius.sidebar.collapsed";

export function Shell({ unreachable, onRetry }: { unreachable: boolean; onRetry: () => void }) {
  const connection = useConnection();
  const reconnect = useReconnect();
  const [collapsed, setCollapsed] = useState(() => readFlag(COLLAPSED_KEY));
  const [commandOpen, setCommandOpen] = useState(false);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandOpen((open) => !open);
      }
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
          <Outlet />
        </main>
      </div>
      <CommandBar open={commandOpen} onOpenChange={setCommandOpen} />
      <FloatingChat />
    </div>
  );
}
