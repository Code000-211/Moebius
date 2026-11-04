/**
 * Deleting a chat: the question and the request, shared by the chat's top bar (where the delete is the danger button
 * beside the other actions) and the rows of the list of chats. It says what goes with the chat, and, for one that is
 * going, that it is stopped first. When the chat that is open is deleted the page goes back to the Chat page's start.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { deleteChat } from "../api/calls/chat";
import { keys } from "../api/keys";
import type { Chat } from "../api/types/chat";
import { ConfirmDelete } from "../ui/page/ConfirmDelete";
import { isWorking } from "./format";

export function DeleteChat({
  chat,
  open,
  onOpenChange,
  openId,
}: {
  chat: Chat;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The chat the page has open, so that deleting that one leaves its page. */
  openId: number | null;
}) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const going = chat.ended_at === null;
  return (
    <ConfirmDelete
      open={open}
      onOpenChange={onOpenChange}
      title="Delete this chat?"
      onConfirm={async () => {
        await deleteChat(chat.id);
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: keys.chats }),
          queryClient.invalidateQueries({ queryKey: keys.snapshot }),
        ]);
        if (openId === chat.id) void navigate("/chat", { replace: true });
      }}
    >
      {`${chat.title || "This chat"} is removed with its messages, the changes it offered and its branch. A pull request already published stays on the forge.`}
      {going &&
        (isWorking(chat.status)
          ? " It is working now, so it is stopped first."
          : " Its agent is stopped first.")}
    </ConfirmDelete>
  );
}
