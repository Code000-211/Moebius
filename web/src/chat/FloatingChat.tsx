/**
 * The floating chat (the desktop app's `float_chat`): a round button at the bottom right of every page that opens a
 * sheet with the Moebius agent over the page. The sheet is a borderless card down the right that leaves the page
 * recognisable behind it; Escape, or the control that minimises it, closes it and the focus goes back to the button
 * that opened it. Minimising turns it back into the button and the chat stays across pages. Every message carries
 * where the person is, which only the agent is told; the first one starts a chat of origin `assistant`, in the
 * repository of the page when it has one, and the later ones go to the same chat. That chat is a chat of the Chat page
 * like any other — listed there, unread until it is read in one place or the other.
 * A chat whose agent went away is taken up again with what was said in it, as on the Chat page; one the agent ended
 * is left behind, and the next message starts a new conversation.
 */
import { ArrowSquareOutIcon, ChatCircleDotsIcon, PlusIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useLocation } from "react-router";
import { snapshot as readSnapshot } from "../api/calls/board";
import { markSeen, chats as readChats, say, startChat } from "../api/calls/chat";
import { keys } from "../api/keys";
import type { Chat } from "../api/types/chat";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";
import { Notice } from "../ui/page/Notice";
import { SheetFrame, SheetOpener, useSheetBehaviour } from "../ui/page/Sheet";
import { useSheetSlot } from "../ui/page/SheetSlot";
import { Composer } from "./Composer";
import { isOpen, isWorking, statusTag, takesMessage } from "./format";
import { pageContext } from "./pageContext";
import { seqToMark } from "./readMarkers";
import { Starters } from "./Starters";
import { ThreadView } from "./ThreadView";
import { useThread } from "./useThread";

export const FLOATING_AGENT = "Moebius";

export function FloatingChat() {
  const queryClient = useQueryClient();
  const location = useLocation();
  const snapshot = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  // The chat and the package sheet share one slot, so showing this one minimises the other (which keeps its state).
  const slot = useSheetSlot();
  const shown = slot.active === "chat";
  const [chatId, setChatId] = useState<number | null>(null);
  const [message, setMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState(0);
  const [error, setError] = useState<string | null>(null);
  // The thread is read for as long as the chat it belongs to is remembered, not only while the sheet is shown:
  // minimising it and opening it again finds the conversation where it was, not an empty box refilled from the server.
  const thread = useThread(chatId);
  // What the composer holds right now, which the closure of a send cannot see: what is typed while an answer is on its
  // way is the next question, not something to clear away with the one that was sent.
  const held = useRef(message);
  /** How far the chat open in the sheet has been marked read; 0 again for the next conversation. */
  const marked = useRef({ id: 0, seq: 0 });
  const write = (value: string) => {
    held.current = value;
    setMessage(value);
  };

  const chat = chats.data?.find((each) => each.id === chatId);
  // A chat that was in the list and is not any more was deleted (here, or by another client): the sheet starts fresh,
  // since a message to it would only be refused. One the list has not caught up with yet was never in it.
  const listed = useRef(new Set<number>());
  for (const each of chats.data ?? []) listed.current.add(each.id);
  const gone =
    chatId !== null && chats.data !== undefined && chat === undefined && listed.current.has(chatId);
  useEffect(() => {
    if (!gone) return;
    setChatId(null);
    setError(null);
  }, [gone]);
  // A chat that was just started is not in the list yet, so while its record is on the way the message still goes to
  // it rather than opening a second chat.
  const writesHere = chat ? takesMessage(chat.status) : chatId !== null;
  // Whether the agent is still up: a message to a chat whose agent went away starts that agent again on the same chat.
  const up = chat !== undefined && isOpen(chat.status);
  // Where the person is now: sent with every message, and said under the sheet's own name, because the page behind it
  // is partly covered and is the thing being asked about.
  const where = pageContext(location.pathname, snapshot.data);
  const working = chat !== undefined && isWorking(chat.status);
  // The same words the Chat page uses for the state of the chat, so the sheet does not look alive when its agent is
  // gone: the next message takes the chat up again rather than starting a new one.
  const tag = chat ? statusTag(chat.status, chat.asks) : null;

  // Escape and the minimise control hide the sheet, and the focus goes back to the button that opened it.
  const { hide: release, show: take } = slot;
  const hide = useCallback(() => release("chat"), [release]);
  const opener = useSheetBehaviour(shown, hide);

  // Marks the conversation read once an answer is held, not at every streamed piece of it, and only while the sheet is
  // open. An answer that arrived after it was minimised is still unread, which is what the Chat page's count is for.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when the answer, the marker or the events move
  useEffect(() => {
    if (!shown || chatId === null) return;
    const seq = seqToMark(chat, thread.lastSeq);
    if (seq === null || (marked.current.id === chatId && seq <= marked.current.seq)) return;
    marked.current = { id: chatId, seq };
    queryClient.setQueryData<Chat[]>(keys.chats, (held) =>
      held?.map((each) =>
        each.id === chatId ? { ...each, seen: Math.max(each.seen, seq) } : each,
      ),
    );
    markSeen(chatId, seq).catch(() => {
      marked.current = { id: chatId, seq: 0 };
    });
  }, [shown, chatId, chat?.answered, chat?.seen, thread.lastSeq]);

  async function send() {
    const text = message.trim();
    if (text === "" || sending) return;
    setSending(true);
    setError(null);
    setSent((n) => n + 1);
    try {
      if (chatId !== null && writesHere) {
        await say(chatId, text, where.text);
        // Taking a chat up again says something new in its tag and in the list of chats, so both are read again
        // instead of waiting for the events to say so.
        if (!up) await queryClient.invalidateQueries({ queryKey: keys.chats });
      } else {
        const run = await startChat({
          agent: FLOATING_AGENT,
          message: text,
          context: where.text,
          origin: "assistant",
          ...(where.repository === null ? {} : { repository: where.repository }),
        });
        setChatId(run.id);
        await queryClient.invalidateQueries({ queryKey: keys.chats });
      }
      if (held.current.trim() === text) write("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Couldn't send the message. Try again.");
    }
    setSending(false);
  }

  if (!shown) {
    return (
      <SheetOpener
        openerRef={opener}
        label="Open the floating chat"
        title="Ask the agent about this page"
        onOpen={() => take("chat")}
        icon={<ChatCircleDotsIcon aria-hidden="true" className="size-5" weight="fill" />}
        className="right-6"
      />
    );
  }

  return (
    <SheetFrame
      label="Floating chat"
      title="Moebius agent"
      tag={tag && <Tag tone={tag.tone}>{tag.label}</Tag>}
      line={`viewing ${where.name}`}
      minimiseLabel="Minimise the floating chat"
      onMinimise={hide}
      className="top-6 right-6 bottom-24 w-96"
      // Both belong to a conversation that exists: nothing is left behind, and nowhere else to open, when this is
      // one nobody has spoken in yet.
      actions={
        chatId !== null && (
          <>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="New conversation"
              title="New conversation"
              onClick={() => {
                setChatId(null);
                setError(null);
              }}
            >
              <PlusIcon aria-hidden="true" className="size-5" weight="bold" />
            </Button>
            <Button asChild variant="ghost" size="icon" title="Open in Chat">
              <Link to={`/chat/${chatId}`} aria-label="Open in Chat" onClick={hide}>
                <ArrowSquareOutIcon aria-hidden="true" className="size-5" weight="bold" />
              </Link>
            </Button>
          </>
        )
      }
    >
      <div className="min-h-0 flex-1" data-testid="floating-conversation">
        {/* A conversation already read stays on screen: a failed read shows its reason above it, not instead of it,
            and a read that never succeeded shows the reason where the bars would have been. */}
        {chatId !== null && thread.error ? <Notice tone="error">{thread.error}</Notice> : null}
        {chatId !== null && thread.error && !thread.loaded ? null : (
          <ThreadView
            entries={chatId === null ? [] : thread.entries}
            working={working}
            sent={sent}
            // A chat that has not been started yet is empty, not still being read: it gets the starters, not bars.
            loading={chatId !== null && !thread.loaded}
            loadingMessage="Reading the chat"
            hasOlder={chatId !== null && thread.hasOlder}
            olderLoading={thread.olderLoading}
            olderError={chatId !== null ? thread.olderError : null}
            loadOlder={thread.loadOlder}
            empty="Ask the agent about this page, or start from one of these. It is told where you are."
            // The starters are for the moment before anything is written; once a message is on its way they are in the way.
            emptyAction={message.trim() === "" ? <Starters onPick={write} /> : undefined}
            compact
          />
        )}
      </div>
      <Composer
        value={message}
        onChange={write}
        onSend={() => void send()}
        canSend={message.trim() !== "" && !sending}
        error={error}
        placeholder="Ask the agent"
        autoFocus
        compact
      />
    </SheetFrame>
  );
}
