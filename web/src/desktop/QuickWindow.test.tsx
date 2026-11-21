/**
 * What the small window of the global shortcut owes a person: Enter sends the question to the Moebius agent or makes the
 * issue in the chosen project and then asks the app to show the chat or the card; Escape leaves without sending; the
 * shortcut is changed by pressing the new keys; and the main window opens what it is told to open.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createIssue } from "../api/calls/board";
import { startChat } from "../api/calls/chat";
import { listProjects } from "../api/calls/projects";
import { OPEN_EVENT, type QuickBridge, SHOWN_EVENT } from "./bridge";
import { OpenRequests } from "./OpenRequests";
import { QuickWindow, repositoryOf, splitIssue } from "./QuickWindow";

vi.mock("../api/calls/board", () => ({ createIssue: vi.fn() }));
vi.mock("../api/calls/chat", () => ({ startChat: vi.fn() }));
vi.mock("../api/calls/projects", () => ({ listProjects: vi.fn() }));

const project = (id: number, name: string, repositories: number[], home = repositories[0]) => ({
  id,
  name,
  repositories: repositories.map((r) => ({ id: r, name: `repo-${r}`, home: r === home })),
});

let quick: QuickBridge & {
  finish: ReturnType<typeof vi.fn>;
  setShortcut: ReturnType<typeof vi.fn>;
};

beforeEach(() => {
  localStorage.clear();
  vi.mocked(listProjects).mockResolvedValue([
    project(1, "Web", [10, 11], 11),
    project(2, "Docs", [20]),
  ] as never);
  vi.mocked(startChat).mockResolvedValue({ id: 42 } as never);
  vi.mocked(createIssue).mockResolvedValue({ id: 7 } as never);
  quick = {
    shortcut: vi.fn().mockResolvedValue({ shortcut: "Ctrl+Shift+Space", error: null }),
    setShortcut: vi.fn().mockImplementation(async (s: string) => s),
    finish: vi.fn().mockResolvedValue(undefined),
  };
  window.moebiusDesktop = { quick } as unknown as typeof window.moebiusDesktop;
});

afterEach(() => {
  cleanup();
  window.moebiusDesktop = undefined;
  vi.clearAllMocks();
});

function open() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <QuickWindow />
    </QueryClientProvider>,
  );
}

describe("the pieces", () => {
  it("makes an issue's title of its first line and its description of the rest", () => {
    expect(splitIssue("  Fix it \n\nIt breaks.\nBadly ")).toEqual({
      title: "Fix it",
      body: "It breaks.\nBadly",
    });
    expect(splitIssue("Only a title")).toEqual({ title: "Only a title", body: "" });
  });

  it("makes issues in a project's home repository, or the first it has", () => {
    expect(repositoryOf(project(1, "Web", [10, 11], 11) as never)).toBe(11);
    expect(repositoryOf(project(1, "Web", [10, 11], 99) as never)).toBe(10);
    expect(repositoryOf({ repositories: [] } as never)).toBeNull();
  });
});

describe("QuickWindow", () => {
  it("sends a question to the Moebius agent on Enter and shows the chat", async () => {
    open();
    const field = screen.getByRole("textbox", { name: "Ask Moebius" });
    fireEvent.change(field, { target: { value: "  what is stuck?  " } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(quick.finish).toHaveBeenCalledWith("/chat/42"));
    expect(startChat).toHaveBeenCalledWith({
      agent: "Moebius",
      message: "what is stuck?",
      origin: "assistant",
    });
  });

  it("keeps a new line on Shift+Enter and sends nothing empty", () => {
    open();
    const field = screen.getByRole("textbox", { name: "Ask Moebius" });
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.change(field, { target: { value: "one" } });
    fireEvent.keyDown(field, { key: "Enter", shiftKey: true });
    expect(startChat).not.toHaveBeenCalled();
  });

  it("makes an issue in the chosen project and shows it on the board", async () => {
    open();
    fireEvent.click(screen.getByRole("tab", { name: "New issue" }));
    const field = screen.getByRole("textbox", { name: "The new issue" });
    await screen.findByText("Web");
    fireEvent.change(field, { target: { value: "Add dark mode\nIt is too bright." } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() =>
      expect(quick.finish).toHaveBeenCalledWith("/projects/1?repository=11&card=7"),
    );
    expect(createIssue).toHaveBeenCalledWith(11, "Add dark mode", "It is too bright.");
  });

  it("says what the server refused and stays open", async () => {
    vi.mocked(startChat).mockRejectedValue(new Error("No agent is free."));
    open();
    const field = screen.getByRole("textbox", { name: "Ask Moebius" });
    fireEvent.change(field, { target: { value: "hello" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect((await screen.findByRole("alert")).textContent).toBe("No agent is free.");
    expect(quick.finish).not.toHaveBeenCalled();
  });

  it("closes on Escape without sending, and starts empty when shown again", () => {
    open();
    const field = screen.getByRole("textbox", { name: "Ask Moebius" }) as HTMLTextAreaElement;
    fireEvent.change(field, { target: { value: "half a thought" } });
    act(() => {
      window.dispatchEvent(new Event(SHOWN_EVENT));
    });
    expect(field.value).toBe("");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(quick.finish).toHaveBeenCalledWith(undefined);
    expect(startChat).not.toHaveBeenCalled();
  });

  it("changes the shortcut to the keys pressed after Change", async () => {
    open();
    await screen.findByText("Ctrl+Shift+Space");
    fireEvent.click(screen.getByRole("button", { name: "Change" }));
    fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true, altKey: true });
    await screen.findByText("Ctrl+Alt+K");
    expect(quick.setShortcut).toHaveBeenCalledWith("Ctrl+Alt+K");
    expect(quick.finish).not.toHaveBeenCalled();
  });

  it("says why the app refused a shortcut and keeps the one in force", async () => {
    quick.setShortcut.mockRejectedValue("Ctrl+K is taken by another program");
    open();
    await screen.findByText("Ctrl+Shift+Space");
    fireEvent.click(screen.getByRole("button", { name: "Change" }));
    fireEvent.keyDown(window, { key: "k", code: "KeyK", ctrlKey: true });
    await screen.findByText(/taken by another program/);
  });
});

function Where() {
  return <p data-testid="where">{useLocation().pathname}</p>;
}

describe("OpenRequests", () => {
  it("goes to the path the app sends and ignores what is not a page of the app", () => {
    render(
      <MemoryRouter initialEntries={["/inbox"]}>
        <OpenRequests />
        <Routes>
          <Route path="*" element={<Where />} />
        </Routes>
      </MemoryRouter>,
    );
    const send = (detail: unknown) =>
      act(() => {
        window.dispatchEvent(new CustomEvent(OPEN_EVENT, { detail }));
      });
    send("//evil.example/x");
    send(5);
    expect(screen.getByTestId("where").textContent).toBe("/inbox");
    send("/chat/42");
    expect(screen.getByTestId("where").textContent).toBe("/chat/42");
  });
});
