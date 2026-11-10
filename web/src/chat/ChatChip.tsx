/**
 * The chat's actions, held by a chip: an always-visible small button that says the chat's state in one word and opens,
 * on click, the small panel that holds all of the chat's actions — the terminal, offering what the worktree changed,
 * publishing and ending a grilling, stopping the agent, the model and reasoning choices, and Rename and Delete. In
 * the chat's own composer it stands beside Send (`beside`), saying the state where the message goes and covering none
 * of the conversation; the Terminal view, which has no composer to stand beside, keeps it floating over the bottom
 * right of the conversation. The chip lives inside the chat, so it moves with the chat wherever the page puts it,
 * including when the chat gives way to the package sheet; it replaces the actions row and the
 * trashcan that the page's top bar carried, which keeps only the title now. Its own fill comes from the surface, and
 * its words from the theme's own text colour: a state pill carries no emphasis, so it borrows none from the fill of a
 * clear action.
 */

import { CaretUpIcon } from "@phosphor-icons/react";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import {
  changeChatSettings,
  endChat,
  nameChat,
  offerChat,
  publishGrilling,
  stopRun,
} from "../api/calls/chat";
import { sayError } from "../api/errorText";
import { keys } from "../api/keys";
import type { Chat } from "../api/types/chat";
import { PillSelect } from "../ui/chat/PillSelect";
import { Button } from "../ui/components/Button";
import { Tooltip } from "../ui/components/Tooltip";
import { cn } from "../ui/lib/cn";
import { CONTROL } from "../ui/lib/field";
import { DeleteChat } from "./DeleteChat";
import { agentGone, isWorking, takesMessage } from "./format";
import { useChatModelChoices } from "./useChoices";

/** The agent grillings run with; such a chat has the Publish and End grilling actions on its chip. */
const GRILLING_AGENT = "Grilling-partner";

/**
 * The chip's one word for the chat's state. A grilling that waits says what its waiting is for; every other state is
 * said plainly, word for word, including the ones the status tag leaves unsaid — a chip is always there, so it names
 * every state.
 */
export function chipState(chat: Chat): string {
  const working = isWorking(chat.status);
  if (chat.agent === GRILLING_AGENT) return working ? "Grilling" : "Grilling · ready to publish";
  switch (chat.status) {
    case "starting":
      return "Starting";
    case "running":
      return "Running";
    case "waiting":
      return chat.asks ? "Asks you" : "Waiting";
    case "succeeded":
      return "Ended";
    case "failed":
      return "Failed";
    case "stopped":
      return "Stopped";
    default:
      return chat.status;
  }
}

/**
 * The chip with its panel of the chat's actions. Beside Send (`beside`) it is a control in the row the composer's
 * other buttons are, drawn to their height, with its panel taken out of the row so opening it covers nothing; over
 * the conversation (the Terminal view) it floats at the bottom right, its panel growing upward from it. Whether the
 * panel stands open is the page's, and the page says it to whichever chip it draws, so the panel goes with the chat
 * when the view switches under it and the chip is drawn again.
 */
export function ChatChip({
  chat,
  openId,
  onNotice,
  beside = false,
  shown,
  onShown,
}: {
  chat: Chat;
  /** The chat the page is on, for the way the delete dialog leaves it. */
  openId: number;
  onNotice: (notice: string | null) => void;
  /** Drawn into the composer's row, beside Send, instead of floating over the conversation. */
  beside?: boolean;
  /** Whether the panel of actions stands open — the page's to hold, not the chip's. */
  shown: boolean;
  onShown: (shown: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [confirming, setConfirming] = useState<false | "stop" | "end">(false);
  const [naming, setNaming] = useState(false);
  const [draft, setDraft] = useState(chat.title);
  const [deleting, setDeleting] = useState(false);
  const { model, thinking, modelOptions, levelOptions } = useChatModelChoices(chat);
  const field = useRef<HTMLInputElement>(null);

  const working = isWorking(chat.status);
  const inTerminal = params.get("terminal") === "1";
  const grilling = chat.agent === GRILLING_AGENT;
  // The model and the reasoning level are chosen wherever a message will be answered. While the agent is up it is
  // told them; after it went away there is nobody to tell yet, and the agent made again for the next message starts
  // on them, so the choice is still the person's to make here.
  const tellModel = chat.status === "waiting" || agentGone(chat.status);

  function togglePanel() {
    setDraft(chat.title);
    setNaming(false);
    setConfirming(false);
    onShown(!shown);
  }
  // The name is there to be changed, so the field opens focused with the old name selected.
  useEffect(() => {
    if (naming) field.current?.select();
  }, [naming]);

  const toggleTerminal = () =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        if (inTerminal) next.delete("terminal");
        else next.set("terminal", "1");
        return next;
      },
      { replace: true },
    );

  async function offer() {
    try {
      const change = await offerChat(chat.id);
      onNotice(`Offered as a change on ${change.branch}: it waits in the Inbox, ready to merge.`);
    } catch (e) {
      onNotice(e instanceof Error ? e.message : "Couldn't offer the change. Try again.");
    }
    await queryClient.invalidateQueries({ queryKey: keys.chats });
    await queryClient.invalidateQueries({ queryKey: keys.snapshot });
  }

  async function stop() {
    setConfirming(false);
    try {
      await stopRun(chat.id);
    } catch (e) {
      onNotice(e instanceof Error ? e.message : "Couldn't stop the chat. Try again.");
    }
  }

  async function publish() {
    try {
      await publishGrilling(chat.id);
      onNotice(
        "Published: what the grilling partner left waits in the Inbox. The grilling goes on; publish again to replace what was published.",
      );
    } catch (e) {
      onNotice(e instanceof Error ? e.message : "Couldn't publish the issues. Try again.");
    }
    await queryClient.invalidateQueries({ queryKey: keys.snapshot });
  }

  async function end() {
    setConfirming(false);
    try {
      await endChat(chat.id);
    } catch (e) {
      onNotice(e instanceof Error ? e.message : "Couldn't end the grilling. Try again.");
    }
    await queryClient.invalidateQueries({ queryKey: keys.chats });
    await queryClient.invalidateQueries({ queryKey: keys.snapshot });
  }

  /** Writes the name, unless nothing was changed; Enter and leaving the field both save, Escape leaves. */
  async function saveName() {
    if (!naming) return;
    setNaming(false);
    const title = draft.trim();
    if (title === chat.title) return;
    try {
      const named = await nameChat(chat.id, title);
      queryClient.setQueryData<Chat[]>(keys.chats, (chats) =>
        chats?.map((each) => (each.id === named.id ? named : each)),
      );
    } catch (error) {
      onNotice(sayError(error, "Could not name the chat. Try again."));
    }
  }

  async function change(settings: { model?: string; thinking?: string }) {
    try {
      await changeChatSettings(chat.id, settings);
      onNotice(null);
    } catch (e) {
      onNotice(e instanceof Error ? e.message : "Couldn't change the chat. Try again.");
    }
    await queryClient.invalidateQueries({ queryKey: keys.chats });
  }

  const changedLabel = `Offer changes (${chat.changed_files} ${
    chat.changed_files === 1 ? "file" : "files"
  })`;

  /** A full-width action of the panel; a control without its action keeps the reason in the app's own tooltip. */
  const row = (label: string, action?: () => void, reason = "") =>
    action ? (
      <Button variant="secondary" size="sm" className="w-full" onClick={action}>
        {label}
      </Button>
    ) : (
      <Tooltip content={reason}>
        <span className="w-full">
          <Button variant="secondary" size="sm" className="w-full" disabled>
            {label}
          </Button>
        </span>
      </Tooltip>
    );

  return (
    <div
      className={cn(
        "z-10 flex flex-col items-end",
        beside ? "relative" : "pointer-events-none absolute right-3 bottom-3",
      )}
    >
      {shown && (
        <div
          data-testid="chat-chip-panel"
          className={cn(
            "pointer-events-auto flex w-72 flex-col gap-1 rounded-card bg-surface p-2 shadow-raised",
            // Beside Send the panel is taken out of the row, and opens upward; over the conversation it is in
            // the flow of a wrapper that already floats, so it grows upward from the chip of its own accord.
            beside && "absolute right-0 bottom-full mb-2",
          )}
        >
          {confirming !== false ? (
            <div className="flex flex-wrap items-center gap-2 p-2">
              <span className="text-base text-text">
                {confirming === "stop" ? "Stop this chat?" : "End this grilling?"}
              </span>
              <Button variant="secondary" size="sm" onClick={() => setConfirming(false)}>
                Keep going
              </Button>
              <Button size="sm" onClick={() => void (confirming === "stop" ? stop() : end())}>
                {confirming === "stop" ? "Stop chat" : "End grilling"}
              </Button>
            </div>
          ) : naming ? (
            <div className="p-2">
              <input
                ref={field}
                aria-label="Name of the chat"
                value={draft}
                maxLength={80}
                onChange={(event) => setDraft(event.target.value)}
                onBlur={() => void saveName()}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void saveName();
                  if (event.key === "Escape") setNaming(false);
                }}
                className={cn(CONTROL, "w-full")}
                data-testid="chat-title-field"
              />
            </div>
          ) : (
            <>
              {inTerminal
                ? row("Conversation", toggleTerminal)
                : chat.status === "waiting"
                  ? row("Terminal", toggleTerminal)
                  : row("Terminal", undefined, "The terminal opens once the agent has answered")}
              {chat.changed_files > 0 &&
                (working
                  ? row(changedLabel, undefined, "Offer once the agent has answered")
                  : row(changedLabel, () => void offer()))}
              {grilling &&
                (working
                  ? row(
                      "Publish issues",
                      undefined,
                      "Publish once the grilling partner has answered",
                    )
                  : row("Publish issues", () => void publish()))}
              {grilling && !working && row("End grilling", () => setConfirming("end"))}
              {working && row("Stop", () => setConfirming("stop"))}
              {takesMessage(chat.status) && (
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  <PillSelect
                    label="Model"
                    valueOnly
                    value={model}
                    placeholder="Server default"
                    options={modelOptions}
                    onChange={(value) => void change({ model: value })}
                    disabled={!tellModel}
                    searchable
                  />
                  <PillSelect
                    label="Reasoning"
                    valueOnly
                    value={thinking}
                    placeholder="Not set"
                    options={levelOptions}
                    onChange={(value) => void change({ thinking: value })}
                    disabled={!tellModel}
                  />
                </div>
              )}
              <div className="mt-1 flex flex-col gap-1 border-t border-border pt-2">
                {row("Rename", () => setNaming(true))}
                {row("Delete this chat", () => {
                  onShown(false);
                  setDeleting(true);
                })}
              </div>
            </>
          )}
        </div>
      )}
      <Button
        type="button"
        variant="secondary"
        size={beside ? "md" : "sm"}
        onClick={togglePanel}
        aria-expanded={shown}
        data-testid="chat-chip"
        className="pointer-events-auto rounded-full bg-surface px-3.5 shadow-raised"
      >
        {chipState(chat)}
        <CaretUpIcon className="size-4" aria-hidden="true" />
      </Button>
      <DeleteChat chat={chat} open={deleting} onOpenChange={setDeleting} openId={openId} />
    </div>
  );
}
