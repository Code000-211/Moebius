/**
 * One chat, open: its conversation (virtualised, following the answer; or Pi-Bolt's terminal interface in its place),
 * the change it offered, and the composer with the pill of its place; the agent it is written to is named by the field
 * itself. All of its actions — publishing and ending a grilling, offering its change, stopping the agent, the model
 * and the reasoning level, naming, deleting — live on the chip beside the composer's Send (ChatChip), floating over
 * the bottom right of the conversation only in the Terminal view, where no composer stands. The
 * model and the reasoning level change while the chat waits. Writing to a chat that ended goes on in a new chat that
 * is told this one. Once an answer is held its events are marked read, once per answer.
 */

import { ArrowClockwiseIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { continueChat, markSeen, mergeChange, chats as readChats, say } from "../api/calls/chat";
import { keys } from "../api/keys";
import type { Snapshot } from "../api/types/board";
import type { Chat } from "../api/types/chat";
import { Terminal } from "../terminal/LazyTerminal";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";
import { cn } from "../ui/lib/cn";
import { Empty } from "../ui/page/Empty";
import { FadedLine } from "../ui/page/FadedLine";
import { Notice } from "../ui/page/Notice";
import { ChatChip } from "./ChatChip";
import { Composer } from "./Composer";
import { draftKey, readDraft, saveDraft } from "./drafts";
import { agentGone, endedHint, isOpen, isWorking, takesMessage } from "./format";
import { offeredChange, placeName } from "./places";
import { seqToMark } from "./readMarkers";
import { Starters } from "./Starters";
import { ThreadView } from "./ThreadView";
import { useThread } from "./useThread";

/**
 * The title of an open chat, alone in the top bar now: all of the chat's actions live on its chip, and the chip's one
 * word carries the state, so the bar says only what the chat is called.
 */
export function ChatTitle({ chat }: { chat: Chat }) {
  const name = chat.title || "New chat";
  return (
    <span data-testid="chat-title">
      <FadedLine
        text={name}
        keep="head"
        title={name}
        className="text-base font-semibold text-text"
      />
    </span>
  );
}

function OfferedChange({ chat, snapshot }: { chat: Chat; snapshot: Snapshot | undefined }) {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const change = offeredChange(snapshot, chat.id);
  if (!change) return null;
  const label =
    change.state === "merged" ? "Merged" : change.state === "closed" ? "Closed" : "Change offered";
  async function merge() {
    if (!change) return;
    try {
      await mergeChange(change.id);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Couldn't merge the change. Try again.");
    }
    await queryClient.invalidateQueries({ queryKey: keys.snapshot });
  }
  return (
    <div
      className="flex shrink-0 flex-wrap items-center gap-3 rounded-card bg-surface p-4 shadow-card"
      data-testid="offered-change"
    >
      <Tag
        tone={
          change.state === "open" ? "attention" : change.state === "merged" ? "complete" : "plain"
        }
      >
        {label}
      </Tag>
      <span className="min-w-0 flex-1 break-all font-mono text-sm text-secondary">
        {change.branch}
      </span>
      {error && <span className="break-words text-sm text-error-text">{error}</span>}
      {change.state === "open" && (
        <Button size="sm" onClick={() => void merge()}>
          Merge
        </Button>
      )}
    </div>
  );
}

export function OpenChat({
  id,
  snapshot,
  notice,
  onNotice,
}: {
  id: number;
  snapshot: Snapshot | undefined;
  notice: string | null;
  onNotice: (notice: string | null) => void;
}) {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const queryClient = useQueryClient();
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  const chat = chats.data?.find((each) => each.id === id);
  const thread = useThread(id);
  const [message, setMessage] = useState(() => readDraft(draftKey(id)));
  const [sent, setSent] = useState(0);
  const [sending, setSending] = useState(false);
  // Whether the chip's panel of actions stands open. It is this page's, not the chip's: the composer and the Terminal
  // view draw the chip anew each time the view switches under them, and the panel goes with the chat all the same.
  const [panel, setPanel] = useState(false);
  const marked = useRef(0);
  // What the composer holds right now, which the closure of a send cannot see: an answer takes seconds, and what is
  // typed in those seconds is the next question, not something to throw away with the one that was sent.
  const held = useRef(message);

  // The message is the draft of this chat: leaving for another page and coming back finds it where it was left,
  // and sending it leaves nothing behind.
  const key = draftKey(id);
  function write(value: string) {
    held.current = value;
    setMessage(value);
    saveDraft(key, value);
  }
  function clear(text: string) {
    if (held.current.trim() !== text) return;
    held.current = "";
    setMessage("");
    saveDraft(key, "");
  }

  // Marks the chat read once an answer is held, not at every streamed piece of it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when the answer, the marker or the events move
  useEffect(() => {
    // What this page marked counts as seen, even when a read of the list from before the mark says otherwise.
    const held = chat && { ...chat, seen: Math.max(chat.seen, marked.current) };
    const seq = seqToMark(held, thread.lastSeq);
    if (seq === null || seq <= marked.current) return;
    marked.current = seq;
    queryClient.setQueryData<Chat[]>(keys.chats, (held) =>
      held?.map((each) => (each.id === id ? { ...each, seen: Math.max(each.seen, seq) } : each)),
    );
    markSeen(id, seq).catch(() => {
      marked.current = 0;
    });
  }, [id, chat?.answered, chat?.seen, thread.lastSeq]);

  if (!chat) {
    return (
      <div className="p-4">
        {chats.isSuccess ? (
          <Empty
            title="There is no such chat"
            body="It may have been deleted. Pick another chat from the list at the top, or start a new one."
            action={
              <Button asChild size="sm" variant="secondary">
                <Link to="/chat">Start a new chat</Link>
              </Button>
            }
          />
        ) : (
          <p role="status" className="m-0 text-base text-secondary">
            Reading the chat
          </p>
        )}
      </div>
    );
  }

  const open = isOpen(chat.status);
  const working = isWorking(chat.status);
  // A chat whose agent went away still takes a message: that chat is taken up again, told what was said in it, so the
  // person goes on where they are rather than in a new chat.
  const writesHere = takesMessage(chat.status);
  const gone = agentGone(chat.status);

  // Pi-Bolt's own terminal interface takes the place of the conversation; the chip's button brings it back.
  if (open && params.get("terminal") === "1") {
    return (
      <div className="relative flex min-h-0 flex-1 flex-col gap-3" data-testid="chat-terminal">
        <ChatChip chat={chat} openId={id} onNotice={onNotice} shown={panel} onShown={setPanel} />
        <Terminal path={`/v1/chats/${id}/tui`} autoFocus className="min-h-0 flex-1" />
        <p className="shrink-0 text-sm leading-relaxed text-secondary">
          Pi-Bolt's terminal interface continues this chat. Closing it, or Conversation on the chip,
          goes back.
        </p>
      </div>
    );
  }

  /**
   * Sends `text`, which is what the field holds unless the caller names another message. The failure banner sends
   * nothing: to a chat whose agent went away that asks it to answer what it was answering, and puts no new words in it.
   */
  async function send(text = message.trim()) {
    if (sending || !chat) return;
    setSending(true);
    onNotice(null);
    setSent((n) => n + 1);
    try {
      if (writesHere) {
        await say(chat.id, text);
        clear(text);
        // Taking a chat up again says something new in its tag and in the list of chats, so both are read again
        // instead of waiting for the events to say so.
        if (!open) await queryClient.invalidateQueries({ queryKey: keys.chats });
      } else {
        // A chat that ended for good goes on in a new chat that is told this one.
        const run = await continueChat(chat.id, text);
        clear(text);
        await queryClient.invalidateQueries({ queryKey: keys.chats });
        void navigate(`/chat/${run.id}`);
      }
    } catch (e) {
      onNotice(e instanceof Error ? e.message : "Couldn't send the message. Try again.");
    }
    setSending(false);
  }

  const place = placeName(snapshot, chat);
  // Nothing is said under the composer for a chat that has ended: its chip beside Send already says so, and
  // writing to it is the ordinary thing the composer does.
  const hint = notice
    ? notice
    : working
      ? "The agent is working. Your message goes when it finishes this turn."
      : null;

  return (
    <>
      <div className="relative min-h-0 flex-1">
        <div className="min-h-0 h-full" data-testid="conversation">
          {/* A conversation already read stays on screen: a failed read shows its reason above it, not instead of it. */}
          {thread.error ? <Notice tone="error">{thread.error}</Notice> : null}
          {thread.error && !thread.loaded ? null : (
            <ThreadView
              entries={thread.entries}
              working={working}
              sent={sent}
              loading={!thread.loaded}
              loadingMessage="Reading the chat"
              hasOlder={thread.hasOlder}
              olderLoading={thread.olderLoading}
              olderError={thread.olderError}
              loadOlder={thread.loadOlder}
              // An answer to the agent's question is the person's next message.
              onAnswer={writesHere ? (answer) => void send(answer) : undefined}
              sending={sending}
              // The banner that says what went wrong is where the person is already looking, so it is where going
              // through the same message again is offered — the agent is made again in this chat, told what was said.
              retryAction={
                gone ? (
                  <Button
                    size="sm"
                    variant="danger"
                    // The banner is the error's own fill, so the button of it needs its border to read as a button.
                    className="ring-1 ring-error-border ring-inset"
                    onClick={() => void send("")}
                    disabled={sending}
                  >
                    <ArrowClockwiseIcon aria-hidden="true" className="size-4" />
                    Retry
                  </Button>
                ) : undefined
              }
              empty={thread.loaded ? "Start with one of these, or write your own." : undefined}
              // The starters are for the moment before anything is written; once a message is on its way they are in
              // the way, and picking one would replace what was typed. The chat's state is not part of this: a chat
              // whose agent went away takes these words itself, and one that ended for good starts a new chat told
              // them, so the sentence above them is true wherever the chat has nothing said in it.
              emptyAction={
                thread.loaded && message.trim() === "" ? <Starters onPick={write} /> : undefined
              }
            />
          )}
        </div>
      </div>
      <OfferedChange chat={chat} snapshot={snapshot} />
      <Composer
        value={message}
        onChange={write}
        onSend={() => void send()}
        canSend={message.trim() !== "" && !sending}
        // A chat that takes no message says on the button what pressing it really does: it starts a chat that carries
        // this one on. The chip beside Send is where the state itself is said.
        sendLabel={writesHere ? "Send" : "Continue"}
        sendTitle={writesHere ? undefined : "Starts a new chat, told what was said in this one"}
        // The field names who the message goes to, which is what the agent pill used to say and is the one thing
        // worth reading there. A chat that takes no message has nobody to write to, so the field says why, and what
        // the button does about it.
        placeholder={writesHere ? `Write to ${chat.agent}` : endedHint(chat.status, chat.agent)}
        pills={<Tag>{place ?? "no repository"}</Tag>}
        actions={
          <ChatChip
            chat={chat}
            openId={id}
            onNotice={onNotice}
            beside
            shown={panel}
            onShown={setPanel}
          />
        }
      />
      {hint && (
        <p
          className={cn(
            "shrink-0 break-words text-sm leading-relaxed",
            notice ? "text-text" : "text-secondary",
          )}
          data-testid="chat-hint"
        >
          {hint}
        </p>
      )}
    </>
  );
}
