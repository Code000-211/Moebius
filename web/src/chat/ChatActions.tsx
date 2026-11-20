/**
 * The actions of an open chat that a person looks for by name in its head: Stop while the agent works, Rename and
 * Delete. The chip beside Send keeps them too, with the model and the terminal; these are the same requests, so a
 * person who does not open the chip's panel still finds them. Only the chat's owner (or anyone, for a chat nobody owns)
 * has them, as with the chip.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { nameChat, stopRun } from "../api/calls/chat";
import { sayError } from "../api/errorText";
import { keys } from "../api/keys";
import type { Chat } from "../api/types/chat";
import { Button } from "../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/components/Dialog";
import { CONTROL } from "../ui/lib/field";
import { Notice } from "../ui/page/Notice";
import { DeleteChat } from "./DeleteChat";
import { isWorking } from "./format";

export function ChatActions({ chat }: { chat: Chat }) {
  const queryClient = useQueryClient();
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [draft, setDraft] = useState(chat.title);
  const [error, setError] = useState<string | null>(null);
  if (!chat.mine && chat.owner !== null) return null;

  async function stop() {
    setError(null);
    try {
      await stopRun(chat.id);
    } catch (e) {
      setError(sayError(e, "Couldn't stop the chat. Try again."));
    }
    await queryClient.invalidateQueries({ queryKey: keys.chats });
  }

  async function rename() {
    const title = draft.trim();
    if (title !== chat.title) {
      try {
        const named = await nameChat(chat.id, title);
        queryClient.setQueryData<Chat[]>(keys.chats, (chats) =>
          chats?.map((each) => (each.id === named.id ? named : each)),
        );
      } catch (e) {
        setError(sayError(e, "Could not name the chat. Try again."));
      }
    }
    setRenaming(false);
  }

  return (
    <div className="flex items-center gap-2" data-testid="chat-actions">
      {error && (
        <Notice tone="error" className="min-h-0 py-0.5 text-sm">
          {error}
        </Notice>
      )}
      {isWorking(chat.status) && (
        <Button size="sm" variant="secondary" onClick={() => void stop()}>
          Stop
        </Button>
      )}
      <Button
        size="sm"
        variant="ghost"
        onClick={() => {
          setDraft(chat.title);
          setRenaming(true);
        }}
      >
        Rename
      </Button>
      <Button size="sm" variant="ghost" onClick={() => setDeleting(true)}>
        Delete
      </Button>
      <Dialog open={renaming} onOpenChange={setRenaming}>
        <DialogContent>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void rename();
            }}
            className="flex flex-col gap-4"
          >
            <DialogHeader>
              <DialogTitle>Rename this chat</DialogTitle>
              <DialogDescription>
                Leave the name empty to name it after its first message.
              </DialogDescription>
            </DialogHeader>
            <input
              aria-label="Name of the chat"
              value={draft}
              maxLength={80}
              onChange={(event) => setDraft(event.target.value)}
              className={CONTROL}
            />
            <DialogFooter>
              <Button type="button" variant="secondary" onClick={() => setRenaming(false)}>
                Cancel
              </Button>
              <Button type="submit">Rename</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
      <DeleteChat chat={chat} open={deleting} onOpenChange={setDeleting} openId={chat.id} />
    </div>
  );
}
