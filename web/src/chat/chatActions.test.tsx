// @vitest-environment jsdom
/**
 * The actions of an open grilling chat: what its partner left is published without ending the chat, and the grilling
 * ends with a yes and without a word. A chat that is not a grilling has neither button.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Chat } from "../api/types/chat";
import { ChatActions } from "./OpenChat";

afterEach(cleanup);

const calls = vi.hoisted(() => ({
  publishGrilling: vi.fn(),
  endChat: vi.fn(),
}));
vi.mock("../api/calls/chat", () => ({
  publishGrilling: calls.publishGrilling,
  endChat: calls.endChat,
}));

/** A chat that waits or works, with the fields the actions row reads. */
function chat(status: string, agent = "Coder"): Chat {
  return { id: 3, agent, status, changed_files: 0 } as unknown as Chat;
}

function renderActions(chat: Chat, onNotice: (notice: string) => void = () => {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ChatActions chat={chat} onNotice={onNotice} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the grilling chat's actions", () => {
  it("publishes what the grilling partner left while the chat goes on", async () => {
    calls.publishGrilling.mockResolvedValue({});
    const onNotice = vi.fn();
    renderActions(chat("waiting", "Grilling-partner"), onNotice);
    fireEvent.click(screen.getByRole("button", { name: "Publish issues" }));
    await vi.waitFor(() =>
      expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("waits in the Inbox")),
    );
    expect(calls.publishGrilling).toHaveBeenCalledWith(3);
  });

  it("holds publishing back while the partner works, and offers nothing else while it does", () => {
    renderActions(chat("running", "Grilling-partner"));
    expect(
      (screen.getByRole("button", { name: "Publish issues" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(screen.queryByRole("button", { name: "End grilling" })).toBeNull();
  });

  it("ends the grilling only after a yes", async () => {
    calls.endChat.mockResolvedValue({ id: 3 });
    renderActions(chat("waiting", "Grilling-partner"));
    fireEvent.click(screen.getByRole("button", { name: "End grilling" }));
    expect(screen.getByText("End this grilling?")).toBeTruthy();
    expect(calls.endChat).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "End grilling" }));
    await vi.waitFor(() => expect(calls.endChat).toHaveBeenCalledWith(3));
  });

  it("gets no grilling buttons", () => {
    renderActions(chat("waiting"));
    expect(screen.queryByRole("button", { name: "Publish issues" })).toBeNull();
    expect(screen.queryByRole("button", { name: "End grilling" })).toBeNull();
  });
});
