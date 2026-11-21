// @vitest-environment jsdom
/**
 * The list of earlier conversations in the Moebius agent's panel: the person's own chats of the agent, newest first,
 * each a row that opens it, and a quiet line when there are none.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Chat } from "../api/types/chat";
import { EarlierConversations, earlier } from "./EarlierConversations";

afterEach(cleanup);

const chat = (id: number, extra: Partial<Chat>): Chat => ({
  id,
  agent: "Moebius",
  status: "stopped",
  repository_id: null,
  harness_repository: false,
  started_at: 1_000 + id,
  ended_at: null,
  title: `Chat ${id}`,
  answered: 0,
  seen: 0,
  changed_files: 0,
  asks: false,
  origin: "assistant",
  model: null,
  thinking: null,
  owner: null,
  shared: false,
  mine: true,
  ...extra,
});

describe("earlier conversations", () => {
  it("keeps the person's own chats of the agent, the newest first", () => {
    const list = earlier([
      chat(1, {}),
      chat(2, { origin: "chat" }),
      chat(3, { mine: false }),
      chat(4, {}),
    ]);
    expect(list.map((each) => each.id)).toEqual([4, 1]);
  });

  it("opens a conversation from its row", () => {
    const open = vi.fn();
    render(
      <EarlierConversations
        chats={earlier([chat(1, {}), chat(4, {})])}
        current={null}
        onOpen={open}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Chat 1/ }));
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ id: 1 }));
  });

  it("says there are none, in one line", () => {
    render(<EarlierConversations chats={[]} current={null} onOpen={() => {}} />);
    expect(screen.getByText("No earlier conversations")).toBeTruthy();
  });
});
