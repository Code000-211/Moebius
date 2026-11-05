// @vitest-environment jsdom
/**
 * The two questions of a card's menu. Blocking asks for a reason that may be empty and sends it; deleting says what
 * goes and, on a forge, that the issue stays open and is not brought back, and asks once. A request the server refuses
 * keeps the dialog open with the refusal beside the buttons.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Board, Card, Repository } from "../../api/types/board";
import { BlockDialog, DeleteDialog, ParentDialog, parentChoices } from "./CardDialogs";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const card = {
  id: 7,
  repository_id: 1,
  number: 12,
  title: "Fix the parser",
  body: "",
  author: "person",
  column: "In progress",
  created_at: 0,
  note: null,
  retried_after: null,
  rounds: 0,
  tokens: 0,
  run: null,
  blocked_by: [],
  refusals: 0,
  change: null,
  parent: null,
  counts_as_sub_issue: true,
  closed: false,
  labels: [],
  claimed_by: null,
} satisfies Card;

const repository = (forge: string): Repository => ({
  id: 1,
  name: "demo",
  path: "/r",
  forge: forge as Repository["forge"],
  default_branch: "main",
  github: forge === "github" ? "acme/demo" : null,
  trusted: [],
});

/** A server that answers every request with `status` and `body`, and the requests it was sent. */
function server(status: number, body: string) {
  const fetch = vi.fn(async () => ({
    ok: status >= 200 && status < 300,
    status,
    text: async () => body,
  }));
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

function inClient(ui: React.ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

describe("blocking a card", () => {
  it("sends the reason, trimmed, and says what it did", async () => {
    const fetch = server(200, JSON.stringify(card));
    const onClose = vi.fn();
    const onDone = vi.fn();
    inClient(<BlockDialog card={card} onClose={onClose} onDone={onDone} />);
    expect(screen.getByRole("dialog", { name: "Block #12" })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Reason"), {
      target: { value: "  wait for the design " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Block" }));
    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1));
    const [path, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect([path, init.method, init.body]).toEqual([
      "/v1/cards/7/block",
      "POST",
      JSON.stringify({ reason: "wait for the design" }),
    ]);
    expect(onDone).toHaveBeenCalledWith("Blocked #12. Its runs are stopped.");
    expect(onClose).toHaveBeenCalled();
  });

  it("blocks without a reason, which is optional", async () => {
    const fetch = server(200, JSON.stringify(card));
    inClient(<BlockDialog card={card} onClose={() => {}} onDone={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Block" }));
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
    const [, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(init.body).toBe(JSON.stringify({ reason: "" }));
  });

  it("stays open with the refusal when the server says the card is held already", async () => {
    server(409, "card 7 is held already");
    const onClose = vi.fn();
    const onDone = vi.fn();
    inClient(<BlockDialog card={card} onClose={onClose} onDone={onDone} />);
    fireEvent.click(screen.getByRole("button", { name: "Block" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("card 7 is held already");
    expect(onClose).not.toHaveBeenCalled();
    expect(onDone).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Block #12" })).toBeTruthy();
  });

  it("is closed while there is no card, and Cancel asks nothing of the server", () => {
    const fetch = server(200, "{}");
    const onClose = vi.fn();
    const { rerender } = inClient(<BlockDialog card={card} onClose={onClose} onDone={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
    expect(fetch).not.toHaveBeenCalled();
    rerender(
      <QueryClientProvider client={new QueryClient()}>
        <BlockDialog card={null} onClose={onClose} onDone={() => {}} />
      </QueryClientProvider>,
    );
    expect(screen.queryByRole("dialog", { name: "Block #12" })).toBeNull();
  });
});

describe("deleting a card", () => {
  it("says what goes and that the issue stays open on a forge, then asks once", async () => {
    const fetch = server(200, JSON.stringify(card));
    const onDone = vi.fn();
    inClient(
      <DeleteDialog
        card={card}
        repository={repository("github")}
        onClose={() => {}}
        onDone={onDone}
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "Delete #12" });
    expect(dialog.textContent).toContain("Fix the parser");
    expect(dialog.textContent).toContain("runs");
    expect(dialog.textContent).toContain("worktrees and branches");
    expect(dialog.textContent).toContain("The issue stays open on GitHub");
    expect(dialog.textContent).toContain("does not make a card of it again");
    expect(dialog.textContent).toContain(
      "An open pull request or a pushed branch on GitHub stays too",
    );
    expect(dialog.textContent).toContain("Cards waiting on this one keep waiting");
    expect(fetch).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete card" }));
    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1));
    const [path, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect([path, init.method]).toEqual(["/v1/cards/7", "DELETE"]);
    expect(onDone).toHaveBeenCalledWith(expect.objectContaining({ id: 7 }), "Deleted #12.");
  });

  it("names the forge the repository is on, and says nothing about one without a forge", () => {
    server(200, "{}");
    for (const [forge, name] of [
      ["gitlab", "GitLab"],
      ["forgejo", "Forgejo"],
    ]) {
      const { unmount } = inClient(
        <DeleteDialog
          card={card}
          repository={repository(forge ?? "")}
          onClose={() => {}}
          onDone={() => {}}
        />,
      );
      expect(screen.getByRole("dialog").textContent).toContain(`The issue stays open on ${name}`);
      unmount();
    }
    inClient(
      <DeleteDialog
        card={card}
        repository={repository("none")}
        onClose={() => {}}
        onDone={() => {}}
      />,
    );
    expect(screen.getByRole("dialog").textContent).not.toContain("The issue stays open");
    expect(screen.getByRole("dialog").textContent).not.toContain("pull request");
    expect(screen.getByRole("dialog").textContent).toContain(
      "Cards waiting on this one keep waiting",
    );
  });

  it("stays open with the refusal when the card is gone, and Cancel does not delete", async () => {
    const fetch = server(404, "card 7 not found");
    const onClose = vi.fn();
    inClient(
      <DeleteDialog
        card={card}
        repository={repository("none")}
        onClose={onClose}
        onDone={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Delete card" }));
    expect((await screen.findByRole("alert")).textContent).toContain("card 7 not found");
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});

describe("moving a card to another parent", () => {
  const board = (cards: Card[]): Board => ({
    repository: repository("github"),
    cards,
    round_limit: 5,
    waiting: {},
    changes: [],
  });
  const plan = { ...card, id: 1, number: 1, title: "The plan" };
  const step = { ...card, id: 2, number: 2, title: "A step", parent: 1 };
  const other = { ...card, id: 3, number: 3, title: "Another plan" };
  const substep = { ...card, id: 4, number: 4, title: "A sub-step", parent: 2 };

  it("offers every issue but the card and the issues below it, which would make a circle", () => {
    const b = board([plan, step, other, substep]);
    expect(parentChoices(b, step).map((c) => c.number)).toEqual([1, 3]);
    expect(parentChoices(b, plan).map((c) => c.number)).toEqual([3]);
  });

  it("sends the parent that was chosen, none for no parent, and says what the server said when it refuses", async () => {
    const fetch = server(200, JSON.stringify(step));
    const done = vi.fn();
    inClient(
      <ParentDialog
        board={board([plan, step, other])}
        card={step}
        onClose={() => {}}
        onDone={done}
      />,
    );
    // It starts on the parent it has; Move sends that again, which the server answers by changing nothing.
    const choose = screen.getByLabelText("Parent");
    choose.focus();
    fireEvent.keyDown(choose, { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "#3 Another plan" }));
    fireEvent.click(screen.getByRole("button", { name: "Move" }));
    await waitFor(() => expect(done).toHaveBeenCalledWith("#2 is a sub-issue of #3."));
    const [url, init] = fetch.mock.calls[0] as unknown as [
      string,
      { method: string; body: string },
    ];
    expect([url, init.method, JSON.parse(init.body)]).toEqual([
      "/v1/cards/2/parent",
      "PUT",
      { parent: 3 },
    ]);
  });

  it("keeps the dialog open with the refusal beside the buttons", async () => {
    server(422, "#1 is below #2, so it cannot be its parent");
    const done = vi.fn();
    inClient(
      <ParentDialog board={board([plan, step])} card={step} onClose={() => {}} onDone={done} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Move" }));
    expect((await screen.findByRole("alert")).textContent).toContain("cannot be its parent");
    expect(done).not.toHaveBeenCalled();
  });
});
