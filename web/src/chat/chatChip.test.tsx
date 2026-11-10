// @vitest-environment jsdom
/**
 * The chip of an open chat: it says the chat's state in one word, and the panel it opens holds the chat's actions —
 * what its partner left is published without ending the grilling, the grilling ends with a yes and without a word,
 * the chat is named and deleted. A chat that is not a grilling has neither grilling button. Next to Send it is a
 * control in the row; over the conversation it floats, and it is drawn so its words survive both themes.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Chat } from "../api/types/chat";
import { ChatChip, chipState } from "./ChatChip";

afterEach(cleanup);

const calls = vi.hoisted(() => ({
  publishGrilling: vi.fn(),
  endChat: vi.fn(),
  nameChat: vi.fn(),
  deleteChat: vi.fn(),
  chatModelChoices: {
    model: null,
    thinking: null,
    modelOptions: [],
    levelOptions: [{ value: "low", label: "low" }],
  },
}));
vi.mock("../api/calls/chat", () => ({
  publishGrilling: calls.publishGrilling,
  endChat: calls.endChat,
  nameChat: calls.nameChat,
  deleteChat: calls.deleteChat,
}));
vi.mock("./useChoices", () => ({
  useChatModelChoices: () => calls.chatModelChoices,
}));

/** A chat that waits or works, with the fields the chip reads. */
function chat(status: string, agent = "Coder"): Chat {
  return { id: 3, agent, status, changed_files: 0, title: "" } as unknown as Chat;
}

/** The chip as the page draws it, with the panel state held where the page holds it, one level above the chip. */
function ChipHarness({
  chat: seeded,
  onNotice,
  beside = false,
}: {
  chat: Chat;
  onNotice: (notice: string | null) => void;
  beside?: boolean;
}) {
  const [shown, setShown] = useState(false);
  return (
    <ChatChip
      chat={seeded}
      openId={3}
      onNotice={onNotice}
      beside={beside}
      shown={shown}
      onShown={setShown}
    />
  );
}

function renderChip(
  chat: Chat,
  onNotice: (notice: string | null) => void = () => {},
  beside = false,
) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ChipHarness chat={chat} onNotice={onNotice} beside={beside} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function openPanel(status: string, agent?: string) {
  renderChip(chat(status, agent));
  fireEvent.click(screen.getByTestId("chat-chip"));
}

describe("the chip's one word for the chat's state", () => {
  it("says a grilling that waits is ready to publish, and that a working one is not", () => {
    expect(chipState(chat("waiting", "Grilling-partner"))).toBe("Grilling · ready to publish");
    expect(chipState(chat("running", "Grilling-partner"))).toBe("Grilling");
  });

  it("says every other chat's state, even the ones the status tag leaves unsaid", () => {
    expect(chipState(chat("starting"))).toBe("Starting");
    expect(chipState(chat("running"))).toBe("Running");
    expect(chipState(chat("waiting"))).toBe("Waiting");
    expect(chipState({ ...chat("waiting"), asks: true } as unknown as Chat)).toBe("Asks you");
    expect(chipState(chat("stopped"))).toBe("Stopped");
    expect(chipState(chat("succeeded"))).toBe("Ended");
    expect(chipState(chat("failed"))).toBe("Failed");
  });
});

describe("the chip's panel of actions", () => {
  it("opens on the chip and holds the grilling's actions and Rename and Delete", () => {
    openPanel("waiting", "Grilling-partner");
    expect(screen.getByText("Grilling · ready to publish")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Publish issues" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "End grilling" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Rename" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Delete this chat" })).toBeTruthy();
  });

  it("publishes what the grilling partner left while the chat goes on", async () => {
    calls.publishGrilling.mockResolvedValue({});
    const onNotice = vi.fn();
    renderChip(chat("waiting", "Grilling-partner"), onNotice);
    fireEvent.click(screen.getByTestId("chat-chip"));
    fireEvent.click(screen.getByRole("button", { name: "Publish issues" }));
    await vi.waitFor(() =>
      expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("waits in the Inbox")),
    );
    expect(calls.publishGrilling).toHaveBeenCalledWith(3);
  });

  it("holds publishing back while the partner works", () => {
    openPanel("running", "Grilling-partner");
    expect(screen.getByText("Grilling")).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: "Publish issues" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(screen.queryByRole("button", { name: "End grilling" })).toBeNull();
  });

  it("ends the grilling only after a yes", async () => {
    calls.endChat.mockResolvedValue({ id: 3 });
    openPanel("waiting", "Grilling-partner");
    fireEvent.click(screen.getByRole("button", { name: "End grilling" }));
    expect(screen.getByText("End this grilling?")).toBeTruthy();
    expect(calls.endChat).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "End grilling" }));
    await vi.waitFor(() => expect(calls.endChat).toHaveBeenCalledWith(3));
  });

  it("gets no grilling actions on an ordinary chat, and names every state a chip can sit under", () => {
    openPanel("stopped");
    expect(screen.getByText("Stopped")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Publish issues" })).toBeNull();
    expect(screen.queryByRole("button", { name: "End grilling" })).toBeNull();
  });

  it("names the chat from the panel", async () => {
    calls.nameChat.mockResolvedValue({ id: 3, title: "New name" });
    openPanel("waiting");
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    const field = screen.getByTestId("chat-title-field") as HTMLInputElement;
    fireEvent.change(field, { target: { value: "New name" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await vi.waitFor(() => expect(calls.nameChat).toHaveBeenCalledWith(3, "New name"));
  });

  it("asks before it deletes", () => {
    calls.deleteChat.mockResolvedValue({ id: 3 });
    openPanel("waiting");
    fireEvent.click(screen.getByRole("button", { name: "Delete this chat" }));
    expect(screen.getByRole("dialog", { name: "Delete this chat?" })).toBeTruthy();
    expect(calls.deleteChat).not.toHaveBeenCalled();
  });
});

describe("the chip's place", () => {
  /**
   * The chip once borrowed the fill of a clear action and swapped the fill under it for its own surface: the words
   * kept the colour meant for the ink fill, white-on-white in the light theme and dark-on-dark in the dark one. The
   * own fill of a state pill is the surface, and its words are the theme's own text colour.
   */
  it("is drawn to survive both themes: the words in the text colour, over the surface, borrowing no ink fill", () => {
    renderChip(chat("waiting"));
    const chip = screen.getByTestId("chat-chip");
    const classes = chip.className.split(/\s+/);
    expect(classes, classes.join(" ")).toContain("text-text");
    expect(classes).toContain("bg-surface");
    expect(classes).not.toContain("text-on-ink");
    expect(classes).not.toContain("hover:bg-text/85");
  });

  /** Beside Send the chip is a control of the row, drawn to the row's height, panel out of the row. */
  it("stands beside Send when it is told to, its panel opened upward out of the row", () => {
    renderChip(chat("waiting"), undefined, true);
    const chip = screen.getByTestId("chat-chip");
    const wrapper = chip.parentElement as HTMLElement;
    const wrapperClasses = wrapper.className.split(/\s+/);
    expect(wrapperClasses, wrapperClasses.join(" ")).toContain("relative");
    expect(wrapperClasses).not.toContain("absolute");
    fireEvent.click(chip);
    const panel = screen.getByTestId("chat-chip-panel");
    expect(panel.className.split(/\s+/)).toContain("bottom-full");
  });

  it("keeps floating over the conversation's bottom right where there is no composer to stand beside", () => {
    renderChip(chat("waiting"));
    const chip = screen.getByTestId("chat-chip");
    expect((chip.parentElement as HTMLElement).className.split(/\s+/)).toContain("absolute");
    fireEvent.click(chip);
    expect(screen.getByTestId("chat-chip-panel").className.split(/\s+/)).not.toContain(
      "bottom-full",
    );
  });

  /** The panel is the page's, not the chip's: drawn again in another place, the chip goes on with it open. */
  it("keeps its panel of actions when the view switches and the chip is drawn again in another place", () => {
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <MemoryRouter>
          <ChipMover />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    fireEvent.click(screen.getByTestId("chat-chip"));
    expect(screen.getByTestId("chat-chip-panel")).toBeTruthy();
    // The chip is drawn again elsewhere, the way the page makes when its view switches; the panel goes with it.
    fireEvent.click(screen.getByTestId("move"));
    expect(screen.getByTestId("chat-chip-panel")).toBeTruthy();
  });
});

/**
 * The chip as the page makes it when its view switches: drawn again in another place (the `key` changes, so the chip
 * is mounted anew), with the panel state still held one level above it. The move button makes the switch.
 */
function ChipMover() {
  const [aside, setAside] = useState(false);
  const [shown, setShown] = useState(false);
  return (
    <>
      <ChatChip
        key={aside ? "row" : "float"}
        chat={chat("waiting")}
        openId={3}
        onNotice={() => {}}
        beside={aside}
        shown={shown}
        onShown={setShown}
      />
      <button type="button" data-testid="move" onClick={() => setAside(true)}>
        move
      </button>
    </>
  );
}
