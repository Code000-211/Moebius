/**
 * The Chat page (the desktop app's CH3-D): the column of chats beside one column a page title wide on the open canvas,
 * with the top bar that shows and hides that column and starts a new chat, and under it the form of a new chat
 * (`/chat`) or the chat chosen (`/chat/:id`). Whether the list is shown or hidden is remembered per browser; with
 * nothing remembered it is shown from a wide screen (`lg`) and hidden below it, where it would lie over the chat, and
 * choosing a chat there hides it again.
 */
import { TrashIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { snapshot as readSnapshot } from "../api/calls/board";
import { chats as readChats } from "../api/calls/chat";
import { keys } from "../api/keys";
import { writeFlag } from "../app/storage";
import { Button } from "../ui/components/Button";
import { useNow } from "../ui/lib/useNow";
import { ChatList } from "./ChatList";
import { ChatTopBar } from "./ChatTopBar";
import { DeleteChat } from "./DeleteChat";
import { NewChat } from "./NewChat";
import { ChatActions, ChatTitle, OpenChat } from "./OpenChat";
import { placeName } from "./places";

/** Whether the column of chats is hidden, kept per browser like the shell's own sidebar. */
const COLLAPSED_KEY = "moebius.chatlist.collapsed";

/** The widest screens, where the column sits beside the chat: Tailwind's `lg`. */
function isWide(): boolean {
  return typeof matchMedia !== "function" || matchMedia("(min-width: 1024px)").matches;
}

/** What was chosen for the list in this browser, else shown only where it fits beside the chat. */
function initialListOpen(): boolean {
  try {
    const stored = window.localStorage.getItem(COLLAPSED_KEY);
    if (stored !== null) return stored !== "1";
  } catch {
    // Nothing is remembered; the screen decides.
  }
  return isWide();
}

export function ChatPage() {
  const params = useParams();
  const openId = params.id !== undefined && /^\d+$/.test(params.id) ? Number(params.id) : null;
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  const snapshot = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const now = useNow();
  const [notice, setNotice] = useState<string | null>(null);
  const [listOpen, setListOpen] = useState(initialListOpen);

  // A notice belongs to the chat it was said in.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when another chat is opened
  useEffect(() => setNotice(null), [openId]);

  // Every chat of the server is on this page, including the ones started in the floating window.
  const all = chats.data ?? [];
  const open = openId === null ? undefined : all.find((chat) => chat.id === openId);

  // A chat that was in the list and is not any more was deleted (here, or by another client): its page is left for
  // the Chat page's start. A chat the list has not caught up with yet was never in it, so it is not taken for one.
  const navigate = useNavigate();
  const listed = useRef(new Set<number>());
  for (const chat of all) listed.current.add(chat.id);
  const gone = openId !== null && chats.data !== undefined && !open && listed.current.has(openId);
  useEffect(() => {
    if (gone) void navigate("/chat", { replace: true });
  }, [gone, navigate]);
  const [deleting, setDeleting] = useState(false);

  function toggleList() {
    writeFlag(COLLAPSED_KEY, listOpen);
    setListOpen(!listOpen);
  }

  // Where the column lies over the chat, choosing a chat is done with the column: it is put away, not remembered.
  function chosen() {
    if (!isWide()) setListOpen(false);
  }

  return (
    <div className="relative mx-auto flex h-full w-full max-w-5xl gap-4" data-testid="chat-page">
      {listOpen && (
        <ChatList
          chats={all}
          openId={openId}
          now={now}
          places={(chat) => placeName(snapshot.data, chat)}
          onCollapse={toggleList}
          onChoose={chosen}
        />
      )}
      <div className="mx-auto flex h-full w-full min-w-0 max-w-3xl flex-col gap-4">
        <ChatTopBar
          chats={all}
          listOpen={listOpen}
          onToggleList={toggleList}
          title={
            open ? (
              <ChatTitle chat={open} onNotice={setNotice} />
            ) : (
              <span className="text-base font-semibold text-text">
                {openId === null ? "New chat" : ""}
              </span>
            )
          }
          actions={
            open ? (
              <>
                <ChatActions chat={open} onNotice={setNotice} />
                {/* The dialog asks and says what it will do, so the bar needs no word of its own: a trashcan in the
                    colours of a destructive action, in a row of controls, is the house way of making room for the
                    title. */}
                <Button
                  variant="danger"
                  size="icon"
                  aria-label="Delete this chat"
                  title="Delete this chat"
                  onClick={() => setDeleting(true)}
                >
                  <TrashIcon aria-hidden="true" className="size-5" weight="bold" />
                </Button>
                <DeleteChat
                  chat={open}
                  open={deleting}
                  onOpenChange={setDeleting}
                  openId={openId}
                />
              </>
            ) : null
          }
        />
        {openId === null ? (
          <NewChat snapshot={snapshot.data} />
        ) : (
          <OpenChat
            key={openId}
            id={openId}
            snapshot={snapshot.data}
            notice={notice}
            onNotice={setNotice}
          />
        )}
      </div>
    </div>
  );
}
