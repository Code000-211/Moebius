/**
 * The server switch at the top of the sidebar, as the mockup draws it: a small square mark, the name of the server
 * and a chevron on a bordered field. In the desktop app the field opens a menu of the app's servers (the open one
 * ticked, the count of each one's Inbox, "cannot be reached" for one that does not answer), and of connecting a server
 * by its address and access token or over SSH, renaming one and removing one, which also deletes its token. The app
 * answers `servers()` with every server's state (it asks each, so it is read every few seconds); opening one shows
 * that server's web app, on the page it was left on (the app keeps a page of every server it opened, so this page
 * stays where it is, hidden). The server asked for is marked at once and pulses until the app has shown it, and a page
 * that is shown again reads the servers again (the app says so with `moebius:shown`). In a browser there is no menu:
 * the field only says which server this is, by the `name` it is given.
 */
import { CaretDownIcon, CheckIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Button } from "../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "../ui/components/Dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "../ui/components/DropdownMenu";
import { Input } from "../ui/components/Input";
import { cn } from "../ui/lib/cn";
import { Field } from "../ui/page/Field";
import { StayCheckbox } from "../ui/page/StayCheckbox";
import { type DesktopBridge, NO_SERVER, type RailServer } from "./bridge";

/** How often the switch asks the app how each server is. */
const REFRESH_MS = 10_000;

const KEY = ["desktop", "servers"] as const;

/** What the app tells a page when it shows it again after the page was hidden behind another server's. */
export const SHOWN_EVENT = "moebius:shown";

/** The character on a server's mark: the first letter or digit of its name. */
export function initial(name: string): string {
  const bare = name.replace(/^[a-z]+:\/\//i, "");
  const first = bare.match(/[\p{L}\p{N}]/u)?.[0];
  return first ? first.toUpperCase() : "?";
}

/** What the badge of a server counts, in words: the sidebar counts the items and the unread notices together. */
export function countWords(server: RailServer): string {
  const part = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);
  const parts = [
    server.items > 0 ? part(server.items, "item", "items") : "",
    server.unread > 0 ? part(server.unread, "notice waiting", "notices waiting") : "",
  ].filter(Boolean);
  return parts.length > 0 ? `${parts.join(" and ")} in the Inbox` : "";
}

/** The small square with the first letter of a server's name. */
function Mark({ name }: { name: string }) {
  return (
    <span
      aria-hidden="true"
      className="flex size-5 shrink-0 items-center justify-center rounded-[5px] bg-accent text-xs font-semibold leading-none text-on-accent"
    >
      {initial(name)}
    </span>
  );
}

const FIELD =
  "flex h-9 min-w-0 flex-1 items-center gap-2 rounded-control px-2 text-left transition-colors hover:bg-row-hover text-base font-semibold text-text";

export function ServerSwitch({ bridge, name }: { bridge?: DesktopBridge; name: string }) {
  const servers = useQuery({
    queryKey: KEY,
    queryFn: () => bridge?.servers() ?? Promise.resolve([]),
    enabled: bridge !== undefined,
    refetchInterval: REFRESH_MS,
  });
  const client = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [renaming, setRenaming] = useState<RailServer | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  // This page was hidden while another server was shown: what it drew of the servers is old.
  useEffect(() => {
    const read = () => void client.invalidateQueries({ queryKey: KEY });
    window.addEventListener(SHOWN_EVENT, read);
    return () => window.removeEventListener(SHOWN_EVENT, read);
  }, [client]);

  if (!bridge) {
    return (
      <div data-testid="server-switch" className={FIELD}>
        <Mark name={name} />
        <span className="min-w-0 truncate">{name}</span>
      </div>
    );
  }

  const list = servers.data ?? [];
  const current = pending !== null ? list.find((s) => s.id === pending) : list.find((s) => s.open);
  const shown = current?.name ?? name;

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
      // The server is gone even when its token could not be deleted; the menu shows what is left.
    }
    await client.invalidateQueries({ queryKey: KEY });
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            data-testid="server-switch"
            aria-label={`Server: ${shown}`}
            aria-busy={pending !== null ? "true" : undefined}
            className={cn(
              FIELD,
              "cursor-pointer transition-colors hover:bg-hover",
              pending !== null && "animate-pulse",
            )}
          >
            <Mark name={shown} />
            <span className="min-w-0 flex-1 truncate">{shown}</span>
            <CaretDownIcon size={14} className="shrink-0 text-secondary" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          className="w-(--radix-dropdown-menu-trigger-width) min-w-0"
        >
          {list.map((server) => (
            <DropdownMenuItem
              key={server.id}
              aria-current={server.open ? "true" : undefined}
              data-reachable={server.reachable}
              onSelect={() => !server.open && open(server)}
            >
              <Mark name={server.name} />
              <span className="min-w-0 flex-1 truncate">{server.name}</span>
              {!server.reachable && (
                <span
                  data-testid="server-unreachable"
                  role="img"
                  aria-label="cannot be reached"
                  title="Cannot be reached"
                  className="size-2 shrink-0 rounded-full bg-muted"
                />
              )}
              {server.inbox > 0 && (
                <span
                  data-testid="server-count"
                  title={countWords(server)}
                  className="shrink-0 text-sm text-secondary tabular-nums"
                >
                  {server.inbox > 99 ? "99+" : server.inbox}
                </span>
              )}
              <CheckIcon aria-hidden="true" className={server.open ? "" : "invisible"} />
            </DropdownMenuItem>
          ))}
          {list.length > 0 && <hr className="my-1 border-hairline" />}
          <DropdownMenuItem onSelect={() => setAdding(true)}>Connect a server</DropdownMenuItem>
          {list.length > 0 && (
            <>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Rename server</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {list.map((server) => (
                    <DropdownMenuItem key={server.id} onSelect={() => setRenaming(server)}>
                      {server.name}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Remove server</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {list.map((server) => (
                    <DropdownMenuItem key={server.id} onSelect={() => void remove(server)}>
                      {server.name}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <AddServer bridge={bridge} open={adding} onOpenChange={setAdding} />
      <RenameServer
        key={renaming?.id ?? "none"}
        server={renaming}
        onClose={() => setRenaming(null)}
        onRename={async (next) => {
          if (!renaming) return;
          await bridge.renameServer(renaming.id, next);
          await client.invalidateQueries({ queryKey: KEY });
        }}
      />
    </>
  );
}

/** The sheet that names a server. An empty name gives back the server's own. */
function RenameServer({
  server,
  onClose,
  onRename,
}: {
  server: RailServer | null;
  onClose: () => void;
  onRename: (name: string) => Promise<void>;
}) {
  const [error, setError] = useState<string | null>(null);
  return (
    <Dialog open={server !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename server</DialogTitle>
          <DialogDescription>
            The name of the server in the switch and in the tray. Leave it empty to use the server's
            own name.
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            const name = String(new FormData(event.currentTarget).get("name") ?? "");
            // A name left as it was shown (it can carry the address that told two servers apart) is not kept.
            if (name === server?.name) return onClose();
            onRename(name).then(onClose, (reason) => setError(String(reason)));
          }}
        >
          <Field label="Name">
            <Input name="name" defaultValue={server?.name ?? ""} maxLength={40} />
          </Field>
          {error !== null && (
            <p role="alert" className="text-sm text-error-text">
              {error}
            </p>
          )}
          <Button type="submit">Save</Button>
        </form>
      </DialogContent>
    </Dialog>
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
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Connect a server</DialogTitle>
          <DialogDescription>
            {ssh
              ? "A host you can log in to with ssh keys: the app opens a tunnel to its server, as moebius connect does."
              : "The address of a Moebius server and an access token of yours. Stay signed in keeps the token in the system's keychain; without it, it stays in memory."}
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
                  : bridge.connect(
                      String(data.get("address")),
                      String(data.get("token")),
                      data.get("stay") === null,
                    ),
              ssh ? host : undefined,
            );
          }}
        >
          {ssh ? (
            <Field label="SSH host">
              <Input name="host" placeholder="user@host" required />
            </Field>
          ) : (
            <>
              <Field label="Address">
                <Input name="address" placeholder="127.0.0.1:7171" required />
              </Field>
              <Field label="Access token">
                <Input name="token" type="password" autoComplete="off" required />
              </Field>
              <StayCheckbox name="stay" />
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
