/**
 * The Chat page (the desktop app's CH3-D): one column of at most 780 px on the open canvas, with the top bar that drops
 * down the list of chats and starts a new one, and under it the form of a new chat (`/chat`) or the chat chosen
 * (`/chat/:id`).
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useParams } from "react-router";
import { snapshot as readSnapshot } from "../api/calls/board";
import { chats as readChats } from "../api/calls/chat";
import { keys } from "../api/keys";
import { ChatTopBar } from "./ChatTopBar";
import { NewChat } from "./NewChat";
import { ChatActions, ChatTitle, OpenChat } from "./OpenChat";
import { placeName } from "./places";
import { askingCount, listedChats } from "./readMarkers";

/** The time now in Unix seconds, kept current every 30 seconds for "5 min ago". */
function useNow(): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

export function ChatPage() {
  const params = useParams();
  const openId = params.id !== undefined && /^\d+$/.test(params.id) ? Number(params.id) : null;
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  const snapshot = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const now = useNow();
  const [notice, setNotice] = useState<string | null>(null);

  // A notice belongs to the chat it was said in.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs when another chat is opened
  useEffect(() => setNotice(null), [openId]);

  const all = chats.data ?? [];
  const listed = listedChats(all, openId);
  const open = openId === null ? undefined : all.find((chat) => chat.id === openId);

  return (
    <div
      className="mx-auto flex h-full w-full max-w-[780px] flex-col gap-[14px]"
      data-testid="chat-page"
    >
      <ChatTopBar
        chats={listed}
        openId={openId}
        now={now}
        places={(chat) => placeName(snapshot.data, chat)}
        asking={askingCount(listed)}
        title={
          open ? (
            <ChatTitle chat={open} snapshot={snapshot.data} />
          ) : (
            <span className="text-sm font-bold text-high">{openId === null ? "New chat" : ""}</span>
          )
        }
        actions={open ? <ChatActions chat={open} onNotice={setNotice} /> : null}
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
  );
}
