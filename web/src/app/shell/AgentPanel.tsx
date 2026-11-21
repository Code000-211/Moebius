/**
 * The Moebius agent's right panel in the shell. The conversation (`chat/AgentChat.tsx`, with the Markdown and the
 * thread it draws) is loaded when the agent is first opened, not with the page, and stays built after that, so a closed
 * panel keeps the conversation and can open itself when an answer arrives.
 */
import { lazy, Suspense, useEffect } from "react";
import { useAgent } from "./AgentContext";

const AgentChat = lazy(() =>
  import("../../chat/AgentChat").then((all) => ({ default: all.AgentChat })),
);

export function AgentPanel() {
  const agent = useAgent();
  // The reply field takes the focus when the panel opens. The chat is loaded the first time, so the field is looked
  // for over the next frames.
  const { open } = agent;
  useEffect(() => {
    if (!open) return;
    let frame = 0;
    let tries = 0;
    const look = () => {
      const field = document.querySelector<HTMLTextAreaElement>(
        '[data-surface="agent-panel"] textarea',
      );
      if (field) field.focus();
      else if (tries++ < 120) frame = requestAnimationFrame(look);
    };
    frame = requestAnimationFrame(look);
    return () => cancelAnimationFrame(frame);
  }, [open]);
  if (!agent.built) return null;
  return (
    <Suspense fallback={null}>
      <AgentChat
        open={agent.open}
        onShow={agent.show}
        onClose={agent.hide}
        takePending={agent.take}
        pending={agent.pending}
        fresh={agent.fresh}
      />
    </Suspense>
  );
}
