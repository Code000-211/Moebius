import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { keys } from "../api/keys";
import type { Snapshot } from "../api/types/board";
import { applyConnected, applyEvent, invalidates } from "./invalidation";

const snapshot: Snapshot = {
  boards: [
    {
      repository: {
        id: 1,
        name: "demo",
        path: "/r",
        forge: "none",
        default_branch: "main",
        github: null,
        trusted: [],
      },
      cards: [
        {
          id: 7,
          repository_id: 1,
          number: 1,
          title: "t",
          body: "",
          author: "person",
          column: "Backlog",
          created_at: 0,
          note: null,
          retried_after: null,
          rounds: 0,
          tokens: 0,
          run: null,
          blocked_by: [],
          refusals: 0,
          change: null,
        },
      ],
      round_limit: 5,
      waiting: {},
      changes: [],
    },
  ],
  harness_changes: [],
  inbox: [],
};

function client() {
  const queryClient = new QueryClient();
  queryClient.setQueryData(keys.snapshot, snapshot);
  queryClient.setQueryData(keys.session, { signed_in: true });
  queryClient.setQueryData(keys.health, { version: "1" });
  return queryClient;
}

const stale = (queryClient: QueryClient, key: readonly unknown[]) =>
  queryClient.getQueryState(key)?.isInvalidated ?? false;

describe("invalidates", () => {
  it("reads the snapshot again for the kinds that change it", () => {
    for (const kind of [
      "card_moved",
      "card_added",
      "card_updated",
      "card_deleted",
      "change_updated",
      "run_started",
      "run_ended",
    ]) {
      expect(invalidates({ kind, data: {} })).toContainEqual(keys.snapshot);
    }
  });

  it("makes what is read about a card stale with the snapshot, and the scripts with their own event", () => {
    expect(invalidates({ kind: "card_moved", data: {} })).toContainEqual(keys.cards);
    expect(invalidates({ kind: "script_changed", data: {} })).toEqual([keys.scripts]);
    const queryClient = client();
    queryClient.setQueryData(keys.history(7), []);
    queryClient.setQueryData(keys.cardRuns(7), []);
    queryClient.setQueryData(keys.script(1, "Not started"), {});
    applyEvent(queryClient, { kind: "run_ended", data: {} });
    expect(stale(queryClient, keys.history(7))).toBe(true);
    expect(stale(queryClient, keys.cardRuns(7))).toBe(true);
    expect(stale(queryClient, keys.script(1, "Not started"))).toBe(false);
    applyEvent(queryClient, { kind: "script_changed", data: {} });
    expect(stale(queryClient, keys.script(1, "Not started"))).toBe(true);
  });

  it("leaves the snapshot alone for kinds it does not show", () => {
    for (const kind of ["run_event", "board_error", "unknown_kind"]) {
      expect(invalidates({ kind, data: {} })).toEqual([]);
    }
  });

  it("makes the ports stale when one opens or closes, and the runs and diffs when a run or change moves", () => {
    for (const kind of ["port_opened", "port_closed"]) {
      expect(invalidates({ kind, data: {} })).toEqual([keys.ports]);
    }
    expect(invalidates({ kind: "run_ended", data: {} })).toEqual(
      expect.arrayContaining([keys.runs, keys.diffs]),
    );
    expect(invalidates({ kind: "change_updated", data: {} })).toContainEqual(keys.diffs);
    const queryClient = client();
    queryClient.setQueryData(keys.run(3), {});
    queryClient.setQueryData(keys.runDiff(3), {});
    queryClient.setQueryData(keys.changeDiff(1), {});
    applyEvent(queryClient, { kind: "run_ended", data: { run: 3 } });
    expect(stale(queryClient, keys.run(3))).toBe(true);
    expect(stale(queryClient, keys.runDiff(3))).toBe(true);
    expect(stale(queryClient, keys.changeDiff(1))).toBe(true);
  });

  it("reads the first-start steps again when a repository comes or goes", () => {
    expect(invalidates({ kind: "repository_added", data: {} })).toContainEqual(keys.start);
  });

  it("reads the usage again when a run ends and the files of the repository whose file was saved", () => {
    expect(invalidates({ kind: "run_ended", data: {} })).toContainEqual(keys.usage);
    expect(invalidates({ kind: "run_event", data: {} })).not.toContainEqual(keys.usage);
    expect(invalidates({ kind: "file_changed", data: { repository: 3, path: "a" } })).toEqual([
      keys.files(3),
    ]);
    expect(invalidates({ kind: "file_changed", data: {} })).toEqual([]);
  });

  it("makes everything stale when events were lost", () => {
    expect(invalidates({ kind: "lagged", data: { missed: 3 } })).toEqual([[]]);
  });
});

describe("applyEvent", () => {
  it("invalidates the snapshot on card_moved and nothing else", () => {
    const queryClient = client();
    applyEvent(queryClient, {
      kind: "card_moved",
      data: { card: 7, from: "Backlog", to: "Not started" },
    });
    expect(stale(queryClient, keys.snapshot)).toBe(true);
    expect(stale(queryClient, keys.health)).toBe(false);
  });

  it("makes the snapshot and what is read about the card stale when a card is deleted", () => {
    const queryClient = client();
    queryClient.setQueryData(keys.history(7), []);
    applyEvent(queryClient, { kind: "card_deleted", data: { card: 7 } });
    expect(stale(queryClient, keys.snapshot)).toBe(true);
    expect(stale(queryClient, keys.history(7))).toBe(true);
    expect(stale(queryClient, keys.health)).toBe(false);
  });

  it("patches the reason of card_waiting into the snapshot", () => {
    const queryClient = client();
    applyEvent(queryClient, { kind: "card_waiting", data: { card: 7, reason: "needs a person" } });
    const patched = queryClient.getQueryData<Snapshot>(keys.snapshot);
    expect(patched?.boards[0]?.waiting).toEqual({ "7": "needs a person" });
    applyEvent(queryClient, { kind: "card_waiting", data: { card: 7, reason: null } });
    expect(queryClient.getQueryData<Snapshot>(keys.snapshot)?.boards[0]?.waiting).toEqual({});
  });

  it("ignores card_waiting for a card of no board and a cache without a snapshot", () => {
    const queryClient = client();
    applyEvent(queryClient, { kind: "card_waiting", data: { card: 99, reason: "x" } });
    expect(queryClient.getQueryData<Snapshot>(keys.snapshot)?.boards[0]?.waiting).toEqual({});
    const empty = new QueryClient();
    applyEvent(empty, { kind: "card_waiting", data: { card: 1, reason: "x" } });
    expect(empty.getQueryData(keys.snapshot)).toBeUndefined();
  });

  it("does not touch the session on lagged", () => {
    const queryClient = client();
    applyEvent(queryClient, { kind: "lagged", data: { missed: 1 } });
    expect(stale(queryClient, keys.snapshot)).toBe(true);
    expect(stale(queryClient, keys.health)).toBe(true);
  });
});

describe("applyConnected", () => {
  it("reads everything again except the session", () => {
    const queryClient = client();
    applyConnected(queryClient);
    expect(stale(queryClient, keys.snapshot)).toBe(true);
    expect(stale(queryClient, keys.health)).toBe(true);
    expect(stale(queryClient, keys.session)).toBe(false);
  });
});

describe("the package events", () => {
  it.each([
    "package_installed",
    "package_removed",
    "package_update_available",
    "extension_update_available",
    "package_check_error",
  ])("makes what the sheet and the Plugins page read stale on %s", (kind) => {
    const keysStale = invalidates({ kind, data: {} });
    expect(keysStale).toContainEqual(keys.packages);
    expect(keysStale).toContainEqual(keys.plugins);
  });

  it.each(["package_update_available", "extension_update_available"])(
    "reads the snapshot again on %s, since the update waits in the inbox",
    (kind) => {
      expect(invalidates({ kind, data: {} })).toContainEqual(keys.snapshot);
    },
  );

  it("does not react to the history entries of a card that are named like inbox items", () => {
    expect(
      invalidates({ kind: "inbox_added", data: { kind: "extension_update" } }),
    ).not.toContainEqual(keys.packages);
  });
});

describe("what a delete makes stale", () => {
  it("reads the lists the deleted thing was shown in, and the snapshot for the changes and inbox items it took", () => {
    const stale = (kind: string, data: Record<string, unknown>) => invalidates({ kind, data });
    expect(stale("chat_deleted", { run: 3 })).toEqual(
      expect.arrayContaining([keys.snapshot, keys.chats]),
    );
    expect(stale("research_deleted", { research: 2 })).toEqual(
      expect.arrayContaining([keys.snapshot, keys.research]),
    );
    expect(stale("grilling_deleted", { grilling: 2 })).toContain(keys.snapshot);
    expect(stale("export_deleted", { export: 1 })).toContain(keys.exports);
    // A replay is shown nowhere in the app.
    expect(stale("replay_deleted", { replay: 1 })).toEqual([]);
  });
});
