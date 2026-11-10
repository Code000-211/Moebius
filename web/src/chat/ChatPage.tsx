/**
 * The Chat page (the former desktop app's CH3-D): the full canvas beside the shell's sidebar, which holds the list of
 * conversations, with the bar on top that says what the chat is, and under it the form of a new chat (`/chat`) or the
 * chat chosen (`/chat/:id`). All of an open chat's actions live on the chip over its conversation's bottom right;
 * the bar on top keeps only the title.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { snapshot as readSnapshot } from "../api/calls/board";
import { chats as readChats } from "../api/calls/chat";
import { keys } from "../api/keys";
import { usePackageSheet } from "../packages/PackageSheetContext";
import { ChatTopBar } from "./ChatTopBar";
import { NewChat } from "./NewChat";
import { ChatTitle, OpenChat } from "./OpenChat";

/** Every chat the Chat page has shown since the app started, so a chat that vanishes from the list is known to be gone rather than new. */
const seenChats = new Set<number>();

export function ChatPage() {
  const params = useParams();
  const openId = params.id !== undefined && /^\d+$/.test(params.id) ? Number(params.id) : null;
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  const snapshot = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const [notice, setNotice] = useState<string | null>(null);
  // The package sheet sits fixed at the right of the window; on narrower windows, where the shell holds no room for
  // it, the chat gives way itself, so the sheet never lies over the conversation.
  const packages = usePackageSheet();

  // A notice belongs to the chat it was said in.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when another chat is opened
  useEffect(() => setNotice(null), [openId]);

  // Every chat of the server is on this page, including the ones started in the floating window.
  const all = chats.data ?? [];
  const open = openId === null ? undefined : all.find((chat) => chat.id === openId);

  // A chat that was in the list and is not any more was deleted (here, or by another client): its page is left for
  // the Chat page's start. A chat the list has not caught up with yet was never in it, so it is not taken for one.
  // The chats the page has seen are kept for the module, not for the component: the page can be mounted again
  // between the click and the answer, and what it keeps must survive that.
  const navigate = useNavigate();
  for (const chat of all) seenChats.add(chat.id);
  const gone = openId !== null && chats.data !== undefined && !open && seenChats.has(openId);
  useEffect(() => {
    if (gone) void navigate("/chat", { replace: true });
  }, [gone, navigate]);

  return (
    <div
      className={`relative flex h-full w-full min-w-0 flex-col ${packages.shown ? "pr-[27.5rem] 2xl:pr-0" : ""}`}
      data-testid="chat-page"
    >
      {/* 820px is the measure the conversation reads at: wide enough that a line of a diff fits beside its words,
          narrow enough that a person's eye finds the next line without running back along it. */}
      <div className="mx-auto flex h-full min-w-0 max-w-[820px] flex-1 flex-col gap-4">
        <ChatTopBar
          title={
            open ? (
              <ChatTitle chat={open} />
            ) : (
              <span className="text-base font-semibold text-text">
                {openId === null ? "New chat" : ""}
              </span>
            )
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
