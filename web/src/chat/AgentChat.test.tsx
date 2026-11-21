// @vitest-environment jsdom
/**
 * What the Moebius agent's panel offers under an answer. "Approve plan" says the agent proposed a plan; nothing in a
 * chat says that, so no answer gets it — not a greeting, not a clarifying question. The one thing the agent can mark
 * is a question of its question tool, which carries the options it offers: those are drawn as a card, and the choice
 * goes back as the person's next message.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { VirtuosoMockContext } from "react-virtuoso";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Chat } from "../api/types/chat";
import type { Entry } from "./conversation";
import type { Thread } from "./useThread";

const held = vi.hoisted(() => ({
  entries: [] as Entry[],
  said: [] as string[],
  thread: null as (Thread & { entries: Entry[] }) | null,
}));

vi.mock("../api/calls/board", () => ({ snapshot: async () => ({}) }));
vi.mock("../api/calls/chat", () => ({
  markSeen: async () => undefined,
  chats: async () => [
    {
      id: 7,
      agent: "Moebius",
      status: "waiting",
      repository_id: null,
      harness_repository: false,
      started_at: 1,
      ended_at: null,
      title: "hi",
      answered: 3,
      seen: 3,
      changed_files: 0,
      asks: true,
      origin: "assistant",
      model: null,
      thinking: null,
      owner: null,
      shared: true,
      mine: true,
    } satisfies Chat,
  ],
  startChat: async () => ({ id: 7 }),
  say: async (_id: number, text: string) => {
    held.said.push(text);
  },
}));
vi.mock("./useThread", () => {
  const loadOlder = () => undefined;
  return {
    // A new object only when the entries are new, as the real hook's thread is.
    useThread: () => {
      if (held.thread?.entries !== held.entries)
        held.thread = {
          entries: held.entries,
          lastSeq: 3,
          loaded: true,
          error: null,
          hasOlder: false,
          olderLoading: false,
          olderError: null,
          loadOlder,
        };
      return held.thread;
    },
  };
});

const { AgentChat } = await import("./AgentChat");

afterEach(() => {
  cleanup();
  held.said.length = 0;
});

const person: Entry = { at: 1, seq: 1, kind: "person", said: "said", message: "hi" };

function panel() {
  let pending: string | null = "hi";
  render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        {/* jsdom has no layout: without a size the list draws no turn. */}
        <VirtuosoMockContext.Provider value={{ viewportHeight: 600, itemHeight: 100 }}>
          <AgentChat
            open
            onShow={() => undefined}
            onClose={() => undefined}
            pending={pending}
            takePending={() => {
              const text = pending;
              pending = null;
              return text;
            }}
          />
        </VirtuosoMockContext.Provider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the panel under the agent's last answer", () => {
  it("offers no Approve plan and no Reply under a plain question", async () => {
    held.entries = [person, { at: 2, seq: 2, kind: "text", text: "Hello! What can I do for you?" }];
    panel();
    await screen.findByText("Hello! What can I do for you?");
    await waitFor(() => expect(held.said).toEqual([]));
    expect(screen.queryByRole("button", { name: "Approve plan" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Reply" })).toBeNull();
  });

  it("offers the options of the question tool, and sends the chosen one", async () => {
    held.entries = [
      person,
      { at: 2, seq: 2, kind: "text", text: "I can add a version check." },
      {
        at: 3,
        seq: 3,
        kind: "tool",
        call: {
          id: "q1",
          tool: "ask_user_question",
          input: {
            type: "questions",
            questions: [
              {
                header: "Plan",
                question: "Shall I add the version check?",
                multiple: false,
                options: [
                  { label: "Add it", description: "Now", recommended: true },
                  { label: "Leave it", description: "Skip", recommended: false },
                ],
              },
            ],
          },
          check: null,
          mcp: null,
          execution: null,
          output: "",
        },
      },
    ];
    panel();
    const card = await screen.findByTestId("question");
    expect(screen.queryByRole("button", { name: "Approve plan" })).toBeNull();
    fireEvent.click(within(card).getByLabelText(/Leave it/));
    fireEvent.click(within(card).getByText("Send answer"));
    await waitFor(() => expect(held.said).toEqual(['My answers:\n1. Plan: "Leave it"']));
  });
});
