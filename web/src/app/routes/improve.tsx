/** The routes of the app's area: chat with an agent. Pages are fetched when their route opens. */
import { lazy } from "react";

const ChatPage = lazy(() =>
  import("../../chat/ChatPage").then((all) => ({ default: all.ChatPage })),
);

/** The routes of this area, as children of the shell's route. */
export const improveRoutes = [
  { path: "chat", element: <ChatPage /> },
  { path: "chat/:id", element: <ChatPage /> },
];
