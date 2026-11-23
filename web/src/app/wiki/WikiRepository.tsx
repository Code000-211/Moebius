/**
 * Where the wiki's pages are kept. A server with no `[wiki] url` keeps them in a repository of its own, which works
 * at once; an owner is told so above the tree and can connect a git repository to back them up and share them. The
 * address is checked and the history pushed by the server, which then uses the repository with no restart. A person
 * who is not an owner sees nothing: they cannot change it. The same dialog is on the Settings page, and a wiki the
 * server could not open at start says why here and offers the same way out.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { connectWiki, wikiStatus } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Input } from "../../ui/components/Input";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { useToast } from "../../ui/page/Toast";
import { useRole } from "../shell/session";

/** How the wiki is kept, as `GET /v1/wiki/status` says; undefined while it is read or when the server cannot say. */
export function useWikiStatus() {
  const query = useQuery({ queryKey: keys.wikiStatus, queryFn: wikiStatus, retry: false });
  return query.data && typeof query.data.state === "string" ? query.data : undefined;
}

/** The notice above the tree of pages: for an owner, a wiki that is kept on this server only or that could not be opened. */
export function WikiRepositoryNotice() {
  const status = useWikiStatus();
  const [open, setOpen] = useState(false);
  if (useRole() !== "owner" || !status || status.state === "remote") return null;
  const unavailable = status.state === "unavailable";
  return (
    <>
      <Notice
        tone={unavailable ? "error" : "ok"}
        action={
          <Button size="sm" variant="secondary" onClick={() => setOpen(true)}>
            Use a git repository…
          </Button>
        }
      >
        {unavailable
          ? `The wiki could not be opened: ${status.problem ?? "unknown reason"}.`
          : "The pages are kept on this server only. Connect a git repository to back them up and share them."}
      </Notice>
      <ConnectWikiDialog open={open} onOpenChange={setOpen} />
    </>
  );
}

/** Asks for the address of the repository and connects it; the pages are read again once it is. */
export function ConnectWikiDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const toast = useToast();
  const [url, setUrl] = useState("");
  const connect = useMutation({
    mutationFn: () => connectWiki(url.trim()),
    onSuccess: async () => {
      setUrl("");
      onOpenChange(false);
      toast({ kind: "confirmation", text: "The wiki is kept in the repository now." });
      await queryClient.invalidateQueries({ queryKey: keys.wiki });
    },
  });
  const ready = url.trim() !== "" && !connect.isPending;
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) connect.reset();
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (ready) connect.mutate();
          }}
        >
          <DialogHeader>
            <DialogTitle>Use a git repository</DialogTitle>
            <DialogDescription>
              The server checks that it can reach the repository. An empty one gets the pages
              written so far, and every edit after that is pushed to it. One that has pages of its
              own is used when this wiki has none; when both have pages nothing is merged, and you
              are told.
            </DialogDescription>
          </DialogHeader>
          <Field
            label="Repository address"
            error={
              connect.isError
                ? sayError(connect.error, "The repository could not be connected.")
                : undefined
            }
          >
            <Input
              value={url}
              placeholder="https://github.com/acme/wiki"
              autoComplete="off"
              spellCheck={false}
              className="font-mono text-sm"
              onChange={(event) => setUrl(event.target.value)}
            />
          </Field>
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!ready}>
              {connect.isPending ? "Connecting" : "Connect"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
