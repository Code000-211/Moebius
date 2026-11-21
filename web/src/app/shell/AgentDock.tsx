/**
 * The quiet "Ask Moebius" field at the bottom of the page's panel. Enter, or the arrow beside the field, hands the
 * message to the agent's panel and opens it; the field is gone while the panel is open and on Settings, and comes back,
 * with the focus, when the panel closes. It shows no key hint.
 */
import { ArrowBendDownLeftIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../ui/components/Button";
import { isImeConfirmation } from "../../ui/lib/imeComposition";
import { useAgent } from "./AgentContext";

export function AgentDock() {
  const agent = useAgent();
  const [message, setMessage] = useState("");
  const field = useRef<HTMLInputElement>(null);
  const { takeFocus } = agent;
  useEffect(() => {
    if (takeFocus()) field.current?.focus();
  }, [takeFocus]);

  function send() {
    const text = message.trim();
    if (text === "") return;
    setMessage("");
    agent.ask(text);
  }

  return (
    <div data-surface="agent-dock" className="shrink-0 p-4">
      <div className="flex h-10 items-center gap-2 rounded-card border border-hairline-strong bg-panel pl-3 pr-1 focus-within:border-accent">
        <input
          ref={field}
          type="text"
          value={message}
          aria-label="Ask Moebius"
          placeholder="Ask Moebius…"
          autoComplete="off"
          onChange={(event) => setMessage(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter" || event.shiftKey || isImeConfirmation(event.nativeEvent)) {
              return;
            }
            event.preventDefault();
            send();
          }}
          className="h-full min-w-0 flex-1 border-0 bg-transparent text-sm text-text outline-none placeholder:text-muted"
        />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label="Send"
          disabled={message.trim() === ""}
          onClick={send}
        >
          <ArrowBendDownLeftIcon aria-hidden="true" className="size-4" />
        </Button>
      </div>
    </div>
  );
}
