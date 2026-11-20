/**
 * The rail of servers at the far left of the desktop app, on the textured ground (pattern 42): a rounded square per
 * server with the one open marked, the count of its Inbox, a grey dot for one that cannot be reached, and "+" to connect
 * one by its address and access token or over SSH. A right click on a square offers to remove the server, which also
 * deletes its token. The app answers `servers()` with every server's state (it asks each, so it is read every few
 * seconds); opening one shows that server's web app, on the page it was left on (the app keeps a page of every server
 * it opened, so this page stays where it is, hidden). The square asked for is marked at once and pulses until the app
 * has shown the server, and a page that is shown again reads the servers again (the app says so with
 * `moebius:shown`). In a browser nothing is drawn.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Button } from "../ui/components/Button";
import { ContextMenu, useContextMenu } from "../ui/components/ContextMenu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "../ui/components/Dialog";
import { Input } from "../ui/components/Input";
import { Tooltip } from "../ui/components/Tooltip";
import { cn } from "../ui/lib/cn";
import { type DesktopBridge, NO_SERVER, type RailServer } from "./bridge";

/** How often the rail asks the app how each server is. */
const REFRESH_MS = 10_000;

const KEY = ["desktop", "servers"] as const;

/** What the app tells a page when it shows it again after the page was hidden behind another server's. */
export const SHOWN_EVENT = "moebius:shown";

/** The letter a server's square shows: the first of its name, which is its address. */
export function initial(name: string): string {
  return (name.replace(/^[a-z]+:\/\//i, "").match(/[\p{L}\p{N}]/u)?.[0] ?? "?").toUpperCase();
}

export function Rail({ bridge }: { bridge?: DesktopBridge }) {
  const servers = useQuery({
    queryKey: KEY,
    queryFn: () => bridge?.servers() ?? Promise.resolve([]),
    enabled: bridge !== undefined,
    refetchInterval: REFRESH_MS,
  });
  const client = useQueryClient();
  const menu = useContextMenu();
  const [target, setTarget] = useState<RailServer | null>(null);
  const [adding, setAdding] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  // This page was hidden while another server was shown: what it drew of the servers is old.
  useEffect(() => {
    const read = () => void client.invalidateQueries({ queryKey: KEY });
    window.addEventListener(SHOWN_EVENT, read);
    return () => window.removeEventListener(SHOWN_EVENT, read);
  }, [client]);
  if (!bridge) return null;

  function open(server: RailServer) {
    setPending(server.id);
    bridge
      ?.switchServer(server.id)
      .catch(() => undefined)
      .finally(() => {
        setPending(null);
        void client.invalidateQueries({ queryKey: KEY });
      });
  }

  async function remove(server: RailServer) {
    try {
      await bridge?.removeServer(server.id);
    } catch {
      // The server is gone even when its token could not be deleted; the rail shows what is left.
    }
    await client.invalidateQueries({ queryKey: KEY });
  }

  return (
    <nav
      aria-label="Servers"
      data-testid="rail"
      className="flex w-12 shrink-0 flex-col items-center gap-2 px-1.5 pt-1"
    >
      {(servers.data ?? []).map((server) => (
        <ServerSquare
          key={server.id}
          server={server}
          pending={pending}
          onOpen={() => open(server)}
          onMenu={(event) => {
            setTarget(server);
            menu.open(event);
          }}
        />
      ))}
      <Tooltip content="Connect a server" side="right">
        <button
          type="button"
          aria-label="Connect a server"
          onClick={() => setAdding(true)}
          className="flex size-9 cursor-pointer items-center justify-center rounded-control text-secondary transition-colors hover:bg-hover hover:text-text"
        >
          <PlusIcon size={18} />
        </button>
      </Tooltip>
      <ContextMenu
        point={menu.point}
        onClose={menu.close}
        title={target?.name ?? "Server"}
        commands={[
          {
            id: "remove",
            label: "Remove server",
            run: () => {
              if (target) void remove(target);
            },
          },
        ]}
      />
      <AddServer bridge={bridge} open={adding} onOpenChange={setAdding} />
    </nav>
  );
}

function ServerSquare({
  server,
  pending,
  onOpen,
  onMenu,
}: {
  server: RailServer;
  /** The server being opened, which is marked before the app has shown it. */
  pending: string | null;
  onOpen: () => void;
  onMenu: (event: React.MouseEvent) => void;
}) {
  const open = pending === null ? server.open : pending === server.id;
  const loading = pending === server.id;
  const state = server.reachable ? "" : " (cannot be reached)";
  const count = server.inbox > 0 ? `, ${server.inbox} in the Inbox` : "";
  return (
    <Tooltip content={`${server.name}${state}`} side="right">
      <button
        type="button"
        aria-label={`${server.name}${state}${count}`}
        aria-current={open ? "true" : undefined}
        aria-busy={loading ? "true" : undefined}
        data-reachable={server.reachable}
        onClick={open ? undefined : onOpen}
        onContextMenu={onMenu}
        className={cn(
          "relative flex size-9 cursor-pointer items-center justify-center rounded-control font-medium text-secondary transition-colors hover:bg-hover hover:text-text",
          open ? "bg-raised text-text shadow-edge" : "bg-surface",
          loading && "animate-pulse",
        )}
      >
        {initial(server.name)}
        {server.inbox > 0 && (
          <span
            data-testid="rail-count"
            className="absolute -top-1 -right-1 min-w-4 rounded-full bg-attention-fill px-1 text-center text-xs leading-4 text-attention-text"
          >
            {server.inbox > 99 ? "99+" : server.inbox}
          </span>
        )}
        {!server.reachable && (
          <span
            data-testid="rail-unreachable"
            aria-hidden="true"
            className="absolute right-0.5 bottom-0.5 size-2 rounded-full bg-muted"
          />
        )}
      </button>
    </Tooltip>
  );
}

/** The sheet that connects a server by its address and token, or over SSH. */
function AddServer({
  bridge,
  open,
  onOpenChange,
}: {
  bridge: DesktopBridge;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [ssh, setSsh] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [setUp, setSetUp] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function run(call: () => Promise<void>, host?: string) {
    setBusy(true);
    setError(null);
    try {
      await call();
    } catch (reason) {
      const text = String(reason);
      if (text.startsWith(NO_SERVER) && host) setSetUp(host);
      else setError(text);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-4 p-6">
        <DialogHeader>
          <DialogTitle>Connect a server</DialogTitle>
          <DialogDescription>
            {ssh
              ? "A host you can log in to with ssh keys: the app opens a tunnel to its server, as moebius connect does."
              : "The address of a Moebius server and an access token of yours. The token is kept in the system's keychain."}
          </DialogDescription>
        </DialogHeader>
        <div className="flex gap-2">
          <Button variant={ssh ? "ghost" : "secondary"} size="sm" onClick={() => setSsh(false)}>
            Address
          </Button>
          <Button variant={ssh ? "secondary" : "ghost"} size="sm" onClick={() => setSsh(true)}>
            Over SSH
          </Button>
        </div>
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            const host = String(data.get("host") ?? "");
            void run(
              () =>
                ssh
                  ? bridge.connectSsh(host, false)
                  : bridge.connect(String(data.get("address")), String(data.get("token"))),
              ssh ? host : undefined,
            );
          }}
        >
          {ssh ? (
            <Input name="host" aria-label="SSH host" placeholder="user@host" required />
          ) : (
            <>
              <Input name="address" aria-label="Address" placeholder="127.0.0.1:7171" required />
              <Input
                name="token"
                type="password"
                aria-label="Access token"
                autoComplete="off"
                required
              />
            </>
          )}
          {setUp !== null && (
            <div role="alert" className="flex flex-col gap-2 text-sm">
              <p>
                No Moebius server is set up on {setUp}. Set one up there? This installs moebius in
                ~/.moebius and starts a server for you.
              </p>
              <Button
                variant="attention"
                size="sm"
                disabled={busy}
                onClick={() => {
                  const host = setUp;
                  setSetUp(null);
                  void run(() => bridge.connectSsh(host, true), host);
                }}
              >
                Yes, set it up
              </Button>
            </div>
          )}
          {error !== null && (
            <p role="alert" className="text-sm text-error-text">
              {error}
            </p>
          )}
          <Button type="submit" disabled={busy}>
            {busy ? "Connecting…" : "Connect"}
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
