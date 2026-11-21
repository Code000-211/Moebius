/** The routes of the app's area: improve and chat: the experiments, the decision model, and the chat with an agent. Pages are fetched when their route opens. */
import { lazy } from "react";

const ExperimentsPage = lazy(() =>
  import("../experiments/ExperimentsPage").then((all) => ({ default: all.ExperimentsPage })),
);
const DecisionModelPage = lazy(() =>
  import("../decision/DecisionModelPage").then((all) => ({ default: all.DecisionModelPage })),
);
const ChatPage = lazy(() =>
  import("../../chat/ChatPage").then((all) => ({ default: all.ChatPage })),
);

/** The routes of this area, as children of the shell's route. */
export const improveRoutes = [
  { path: "chat", element: <ChatPage /> },
  { path: "chat/:id", element: <ChatPage /> },
  { path: "experiments", element: <ExperimentsPage /> },
  { path: "decision-model", element: <DecisionModelPage /> },
];
