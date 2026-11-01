/**
 * The frame of a signed-in person's page: the sidebar, the banner while the server cannot be reached, and the page.
 * It owns the command bar and its shortcut, the sidebar's collapsed state (kept per browser) and signing out.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Outlet } from "react-router";
import { signOut } from "../../api/calls/session";
import { keys } from "../../api/keys";
import { FloatingChat } from "../../chat/FloatingChat";
import { useConnection, useReconnect } from "../../live/LiveProvider";
import { readFlag, writeFlag } from "../storage";
import { CommandBar } from "./CommandBar";
import { ConnectionBanner } from "./ConnectionBanner";
import { Sidebar } from "./Sidebar";

const COLLAPSED_KEY = "moebius.sidebar.collapsed";

export function Shell({ unreachable, onRetry }: { unreachable: boolean; onRetry: () => void }) {
  const queryClient = useQueryClient();
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

  async function leave() {
    try {
      await signOut();
    } finally {
      queryClient.removeQueries({ predicate: (query) => query.queryKey[0] !== keys.session[0] });
      await queryClient.invalidateQueries({ queryKey: keys.session });
    }
  }

  const offline = unreachable || connection === "reconnecting";

  return (
    <div className="flex h-full bg-ground">
      <Sidebar
        connection={connection}
        collapsed={collapsed}
        onToggle={() => {
          writeFlag(COLLAPSED_KEY, !collapsed);
          setCollapsed(!collapsed);
        }}
        onSearch={() => setCommandOpen(true)}
        onSignOut={() => void leave()}
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
        <main className="min-h-0 flex-1 overflow-y-auto px-[18px] py-[14px]">
          <Outlet />
        </main>
      </div>
      <CommandBar open={commandOpen} onOpenChange={setCommandOpen} />
      <FloatingChat />
    </div>
  );
}
