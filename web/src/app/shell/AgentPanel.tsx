/**
 * The Moebius agent's right panel in the shell. The conversation (`chat/AgentChat.tsx`, with the Markdown and the
 * thread it draws) is loaded when the agent is first opened, not with the page, and stays built after that, so a closed
 * panel keeps the conversation and can open itself when an answer arrives.
 */
import { lazy, Suspense } from "react";
import { useAgent } from "./AgentContext";

const AgentChat = lazy(() =>
  import("../../chat/AgentChat").then((all) => ({ default: all.AgentChat })),
);

export function AgentPanel() {
  const agent = useAgent();
  if (!agent.built) return null;
  return (
    <Suspense fallback={null}>
      <AgentChat
        open={agent.open}
        onShow={agent.show}
        onClose={() => agent.hide(true)}
        takePending={agent.take}
        pending={agent.pending}
      />
    </Suspense>
  );
}
