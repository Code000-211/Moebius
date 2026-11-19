/**
 * What the head of an open chat says about whose it is: its owner gets the button that shares it with the
 * organisation or takes it back, anyone else reads who shared it. A chat nobody owns belongs to everyone and says
 * nothing.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { shareChat } from "../api/calls/chat";
import { sayError } from "../api/errorText";
import { keys } from "../api/keys";
import type { Chat } from "../api/types/chat";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";

export function ShareChat({ chat }: { chat: Chat }) {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (!chat.mine) {
    return chat.owner === null ? null : <Tag>{`Shared by @${chat.owner}`}</Tag>;
  }
  async function toggle() {
    setBusy(true);
    try {
      await shareChat(chat.id, !chat.shared);
      setError(null);
      await queryClient.invalidateQueries({ queryKey: keys.chats });
    } catch (e) {
      setError(sayError(e, "Couldn't change who reads the chat. Try again."));
    }
    setBusy(false);
  }
  return (
    <div className="flex items-center gap-2" data-testid="share-chat">
      {error && (
        <span role="alert" className="text-sm text-error-text">
          {error}
        </span>
      )}
      <Button
        size="sm"
        variant={chat.shared ? "secondary" : "ghost"}
        disabled={busy}
        aria-pressed={chat.shared}
        title={
          chat.shared
            ? "Everyone reads this chat. Stop sharing it."
            : "Let everyone in the organisation read this chat"
        }
        onClick={() => void toggle()}
      >
        {chat.shared ? "Shared" : "Share"}
      </Button>
    </div>
  );
}
