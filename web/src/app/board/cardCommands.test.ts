/**
 * The commands of a card in focus: which are offered for a card as it stands, the key of each, and that each does its
 * own action. The moves are the ones the board allows a person, the same as a drag.
 */
import { describe, expect, it, vi } from "vitest";
import type { Card, InboxItem } from "../../api/types/board";
import { NEW_ISSUE_KEY, SHORTCUT_GROUPS, SHORTCUTS_KEY } from "../shell/palette/shortcuts";
import { type CardActions, cardCommands } from "./cardCommands";

function card(extra: Partial<Card> = {}): Card {
  return {
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
    assignees: [],
    ...extra,
  };
}

function actions() {
  return {
    claim: vi.fn(),
    release: vi.fn(),
    close: vi.fn(),
    reopen: vi.fn(),
    block: vi.fn(),
    unblock: vi.fn(),
    stop: vi.fn(),
    addSubIssue: vi.fn(),
    approve: vi.fn(),
    retry: vi.fn(),
    merge: vi.fn(),
    reject: vi.fn(),
    assignTo: vi.fn(),
  } satisfies CardActions;
}

function item(kind: string): InboxItem {
  return {
    id: 1,
    kind,
    card_id: 7,
    title: "",
    data: { change: 5 },
    resolved_at: null,
    at: 0,
    assignees: [],
  };
}

const ids = (commands: { id: string }[]) => commands.map((c) => c.id);

describe("the commands of a card", () => {
  it("has a command for each person the card can be given to, which gives it or takes it back", () => {
    const act = actions();
    const given = cardCommands(card({ assignees: ["lea"] }), undefined, act, ["lea", "tom"]);
    const lea = given.find((c) => c.id === "unassignee-lea");
    const tom = given.find((c) => c.id === "assignee-tom");
    expect([lea?.label, tom?.label]).toEqual(["Take off @lea", "Assign to @tom"]);
    expect([lea?.key, tom?.key]).toEqual([undefined, undefined]);
    tom?.run();
    lea?.run();
    expect(act.assignTo.mock.calls.map(([, handle, on]) => [handle, on])).toEqual([
      ["tom", true],
      ["lea", false],
    ]);
  });

  it("assigns, blocks, closes and adds a sub-issue, each on its key", () => {
    const act = actions();
    const commands = cardCommands(card(), undefined, act);
    expect(ids(commands)).toEqual(["assign", "block", "close", "sub-issue"]);
    expect(commands.map((c) => c.key)).toEqual(["a", "b", "d", "s"]);
    for (const command of commands) command.run();
    expect(act.claim).toHaveBeenCalledOnce();
    expect(act.block).toHaveBeenCalledOnce();
    expect(act.close).toHaveBeenCalledOnce();
    expect(act.addSubIssue).toHaveBeenCalledOnce();
    expect(commands.filter((c) => c.opensDialog).map((c) => c.id)).toEqual(["block", "sub-issue"]);
  });

  it("takes the assignment back, unblocks and reopens", () => {
    const act = actions();
    const commands = cardCommands(
      card({ claimed_by: "person", closed: true, held: { reason: null, by: "person", at: 0 } }),
      undefined,
      act,
    );
    expect(ids(commands)).toEqual(["unassign", "unblock", "reopen", "sub-issue"]);
    for (const command of commands) command.run();
    expect(act.release).toHaveBeenCalledOnce();
    expect(act.unblock).toHaveBeenCalledOnce();
    expect(act.reopen).toHaveBeenCalledOnce();
  });

  it("stops the run only while there is one", () => {
    const act = actions();
    const live = card({ run: { id: 3, agent: "coder", purpose: "code", status: "running" } });
    const stop = cardCommands(live, undefined, act).find((c) => c.id === "stop");
    expect(stop?.key).toBe("x");
    stop?.run();
    expect(act.stop).toHaveBeenCalledWith(live);
    const done = card({ run: { id: 3, agent: "coder", purpose: "code", status: "succeeded" } });
    expect(ids(cardCommands(done, undefined, actions()))).not.toContain("stop");
  });

  it("moves to Not started to approve an issue that waits", () => {
    const act = actions();
    const waiting = card({ column: "Backlog" });
    const move = cardCommands(waiting, item("approve_issue"), act).find(
      (c) => c.id === "move-Not started",
    );
    expect(move?.label).toBe("Move to Not started");
    move?.run();
    expect(act.approve).toHaveBeenCalledWith(waiting);
  });

  it("moves a stuck card to In progress to retry it", () => {
    const act = actions();
    const stuck = card();
    const move = cardCommands(stuck, item("stuck"), act).find((c) => c.id === "move-In progress");
    move?.run();
    expect(act.retry).toHaveBeenCalledWith(stuck);
  });

  it("merges a change that is ready, or sends it back", () => {
    const act = actions();
    const ready = card({ column: "Ready to merge" });
    const commands = cardCommands(ready, item("ready_to_merge"), act);
    const merge = commands.find((c) => c.id === "move-Done");
    const back = commands.find((c) => c.id === "move-In progress");
    expect(merge?.label).toBe("Move to Done");
    expect(back?.label).toBe("Send back to In progress");
    merge?.run();
    back?.run();
    expect(act.merge).toHaveBeenCalledWith(ready, 5);
    expect(act.reject).toHaveBeenCalledWith(ready, 5);
  });

  it("gives every command of every state a key that is on the shortcuts page, none twice at once", () => {
    const shown = new Set(
      SHORTCUT_GROUPS.flatMap((g) =>
        g.shortcuts.flatMap((s) => s.keys.map((k) => k.toLowerCase())),
      ),
    );
    const states: [Card, InboxItem | undefined][] = [
      [card(), undefined],
      [
        card({ claimed_by: "person", closed: true, held: { reason: null, by: "p", at: 0 } }),
        undefined,
      ],
      [card({ run: { id: 3, agent: "coder", purpose: "code", status: "running" } }), undefined],
      [card({ column: "Backlog" }), item("approve_issue")],
      [card(), item("stuck")],
      [card({ column: "Ready to merge" }), item("ready_to_merge")],
    ];
    for (const [c, i] of states) {
      const commands = cardCommands(c, i, actions());
      for (const command of commands) {
        expect(command.key, command.id).toBeDefined();
        expect(shown.has(command.key ?? ""), command.id).toBe(true);
      }
      const keys = commands.map((command) => command.key);
      expect(new Set(keys).size).toBe(keys.length);
      // `C` and `?` belong to the whole app.
      expect(keys).not.toContain(NEW_ISSUE_KEY);
      expect(keys).not.toContain(SHORTCUTS_KEY);
    }
  });

  it("offers no move where the board lets a person none", () => {
    const moves = cardCommands(card(), undefined, actions()).filter((c) =>
      c.id.startsWith("move-"),
    );
    expect(moves).toEqual([]);
  });
  it("gives every command of every state a key, each on the shortcuts page and none twice at once", () => {
    const shown = new Set(
      SHORTCUT_GROUPS.flatMap((g) =>
        g.shortcuts.flatMap((s) => s.keys.map((k) => k.toLowerCase())),
      ),
    );
    const states: [Card, InboxItem | undefined][] = [
      [card(), undefined],
      [
        card({ claimed_by: "person", closed: true, held: { reason: null, by: "p", at: 0 } }),
        undefined,
      ],
      [card({ run: { id: 3, agent: "coder", purpose: "code", status: "running" } }), undefined],
      [card({ column: "Backlog" }), item("approve_issue")],
      [card(), item("stuck")],
      [card({ column: "Ready to merge" }), item("ready_to_merge")],
    ];
    for (const [c, i] of states) {
      const commands = cardCommands(c, i, actions());
      for (const command of commands) {
        expect(command.key, command.id).toBeDefined();
        expect(shown.has(command.key ?? ""), command.id).toBe(true);
      }
      const keys = commands.map((command) => command.key);
      expect(new Set(keys).size).toBe(keys.length);
      // `C` and `?` belong to the whole app.
      expect(keys).not.toContain(NEW_ISSUE_KEY);
      expect(keys).not.toContain(SHORTCUTS_KEY);
    }
  });
});
