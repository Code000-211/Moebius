/**
 * The board part of the fake server: the data of one repository and the routes the board page calls, with the JSON the
 * real server sends (docs/reference/desktop-api.md: the snapshot, `Card`, `Change`, `Run`, the history, the script of a
 * column, and the decisions). The state is kept per server and changes the way the real one does: approving, merging,
 * sending back, retrying and creating an issue move cards, resolve inbox items and send the events the real server
 * sends; stopping a run ends it as stopped, blocking stops the card's run and holds the card, unblocking releases it, and deleting takes the card, its
 * changes and its inbox items and sends `card_deleted`. The issue tracker works the way the server's does: a parent (cycles
 * refused), blockers (cycles refused, resolved when the blocker is closed), labels, a claim (a second claimant is refused),
 * comments, and closing and reopening, each with the event of its own. A test can also move a card as the board engine would (`moveCard`) and make the next request to a route fail
 * (`refuseNext`), to see the page answer.
 */

export interface ServerEvent {
  kind: string;
  data: unknown;
}

interface FakeCard {
  id: number;
  repository_id: number;
  number: number;
  title: string;
  body: string;
  author: string;
  column: string;
  created_at: number;
  note: string | null;
  retried_after: number | null;
  confirmed: boolean;
  rounds: number;
  tokens: number;
  run: { id: number; agent: string; purpose: string; status: string } | null;
  blocked_by: number[];
  refusals: number;
  change: { id: number; state: string; checks: string; review_verdict: string | null } | null;
  /** Set while a person or the Moebius agent holds the card. */
  held: { reason: string | null; by: string; at: number } | null;
  parent: number | null;
  counts_as_sub_issue: boolean;
  /** The issue was closed; a card in Done counts as closed too (see `view`). */
  closed: boolean;
  labels: string[];
  claimed_by: string | null;
}

interface FakeComment {
  id: number;
  card_id: number;
  author: string;
  body: string;
  at: number;
}

interface FakeChange {
  id: number;
  repository_id: number;
  card_id: number;
  branch: string;
  state: string;
  pull_request: number | null;
  checks: string | null;
  review: string | null;
  head: string | null;
  published_at: number | null;
  checks_head: string | null;
  failed_checks: string | null;
  check_output: string | null;
  harness_repository: boolean;
}

interface FakeInboxItem {
  id: number;
  kind: string;
  card_id: number | null;
  title: string;
  data: Record<string, unknown>;
  resolved_at: number | null;
  at: number;
}

interface HistoryEntry {
  at: number;
  kind: string;
  data: Record<string, unknown>;
}

/** A title longer than any column: one word with no break, and then words. */
export const LONG_TITLE = `${"Supercalifragilistic".repeat(8)} and then a title that goes on for a long while so that it wraps`;

const SCRIPTS: Record<string, string> = {
  backlog: "backlog",
  "not started": "ready",
  "in progress": "code",
  "in review": "review",
  "ready to merge": "merge",
  done: "done",
};

const NOW = 1_790_000_000;

function repository() {
  return {
    id: 1,
    name: "demo",
    path: "/srv/.moebius/repos/demo",
    forge: "github",
    default_branch: "main",
    github: "acme/demo",
    trusted: [],
  };
}

function card(id: number, extra: Partial<FakeCard>): FakeCard {
  return {
    id,
    repository_id: 1,
    number: id,
    title: `Card ${id}`,
    body: "",
    author: "person",
    column: "Not started",
    created_at: NOW - 3600,
    note: null,
    retried_after: null,
    confirmed: false,
    rounds: 0,
    tokens: 0,
    run: null,
    blocked_by: [],
    refusals: 0,
    change: null,
    held: null,
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: [],
    claimed_by: null,
    ...extra,
  };
}

function change(id: number, card_id: number, extra: Partial<FakeChange>): FakeChange {
  return {
    id,
    repository_id: 1,
    card_id,
    branch: `moebius/card-${card_id}`,
    state: "open",
    pull_request: id + 10,
    checks: "success",
    review: null,
    head: "0123456789abcdef",
    published_at: NOW - 600,
    checks_head: "0123456789abcdef",
    failed_checks: null,
    check_output: null,
    harness_repository: false,
    ...extra,
  };
}

export interface BoardFake {
  /** `GET /v1/snapshot`. */
  snapshot(): unknown;
  /** Answers a board route; undefined when `path` is none of them. */
  handle(
    method: string,
    path: string,
    body: unknown,
  ): { status: number; body: unknown } | undefined;
  /** Moves a card as the board engine does, and returns the event the server sends for it. */
  moveCard(id: number, column: string): ServerEvent;
  /** Makes the next request with `method` to `path` fail with `status` and `message`. */
  refuseNext(method: string, path: string, status: number, message: string): void;
  /** The non-GET requests the board routes received, in order. */
  requests: Array<{ method: string; path: string; body: unknown }>;
}

export function createBoardFake(send: (event: ServerEvent) => void): BoardFake {
  const repo = repository();
  let cards: FakeCard[] = [
    card(1, {
      title: "Add a feature",
      column: "In progress",
      rounds: 1,
      tokens: 12_000,
      run: { id: 1, agent: "Coder", purpose: "code", status: "running" },
    }),
    card(2, {
      title: "Fix the login redirect",
      column: "Backlog",
      author: "stranger",
      // Nobody approved an outsider's issue: it counts as no sub-issue of the parent it names.
      counts_as_sub_issue: false,
    }),
    card(3, { title: "Document the API", column: "Not started" }),
    card(4, { title: LONG_TITLE, column: "Not started", blocked_by: [3] }),
    card(5, {
      title: "Speed up the build",
      column: "Ready to merge",
      rounds: 2,
      tokens: 96_000,
      change: { id: 1, state: "open", checks: "success", review_verdict: "approve" },
    }),
    card(6, {
      title: "Stop the flaky test",
      column: "In progress",
      rounds: 5,
      tokens: 480_000,
      run: { id: 2, agent: "Reviewer", purpose: "review", status: "failed" },
    }),
    card(7, { title: "Ship the first release", column: "Done" }),
  ];
  const changes: FakeChange[] = [
    change(1, 5, {}),
    change(2, 7, { state: "merged", pull_request: 9 }),
  ];
  const inbox: FakeInboxItem[] = [
    {
      id: 1,
      kind: "approve_issue",
      card_id: 2,
      title: "Approve: Fix the login redirect",
      data: { reason: "An outside author opened it" },
      resolved_at: null,
      at: NOW - 100,
    },
    {
      id: 2,
      kind: "ready_to_merge",
      card_id: 5,
      title: "Speed up the build is ready to merge",
      data: { change: 1, branch: "moebius/card-5" },
      resolved_at: null,
      at: NOW - 90,
    },
    {
      id: 3,
      kind: "stuck",
      card_id: 6,
      title: "Stop the flaky test is stuck",
      data: { reason: "reached the round limit" },
      resolved_at: null,
      at: NOW - 80,
    },
  ];
  const history = new Map<number, HistoryEntry[]>([
    [
      1,
      [
        { at: NOW - 3000, kind: "moved", data: { from: "Backlog", to: "Not started", note: null } },
        {
          at: NOW - 2000,
          kind: "moved",
          data: { from: "Not started", to: "In progress", note: null },
        },
        { at: NOW - 1900, kind: "run_started", data: { run: 1, agent: "Coder", purpose: "code" } },
      ],
    ],
  ]);
  const runs = new Map<number, unknown[]>([
    [
      1,
      [
        {
          id: 1,
          card_id: 1,
          agent: "Coder",
          harness: "pi-bolt",
          purpose: "code",
          status: "running",
          worktree: "/srv/.moebius/runs/1/worktree",
          started_at: NOW - 1900,
          ended_at: null,
          summary: null,
          repository_id: null,
          harness_repository: false,
          origin: "chat",
          runner: null,
        },
      ],
    ],
  ]);
  const refusals = new Map<string, { status: number; message: string }>();
  const requests: BoardFake["requests"] = [];
  let nextCard = 8;
  let clock = NOW;

  const find = (id: number) => cards.find((c) => c.id === id);
  const note = (id: number, entry: Omit<HistoryEntry, "at">) =>
    history.set(id, [...(history.get(id) ?? []), { at: ++clock, ...entry }]);
  const resolve = (cardId: number, kinds: string[]) => {
    for (const item of inbox) {
      if (item.card_id === cardId && kinds.includes(item.kind) && item.resolved_at === null) {
        item.resolved_at = ++clock;
        note(cardId, {
          kind: "inbox_resolved",
          data: { item: item.id, kind: item.kind, title: item.title },
        });
      }
    }
  };
  function move(target: FakeCard, to: string, why: string | null = null): ServerEvent {
    const from = target.column;
    target.column = to;
    note(target.id, { kind: "moved", data: { from, to, note: why } });
    return { kind: "card_moved", data: { card: target.id, from, to } };
  }
  const view = (c: FakeCard) => ({ ...c, closed: c.closed || c.column === "Done" });
  const comments = new Map<number, FakeComment[]>();
  let nextComment = 1;
  const numbered = (n: number) => cards.find((c) => c.number === n);
  /** The cards below `of`, at any depth. */
  const below = (of: FakeCard): FakeCard[] => {
    const found: FakeCard[] = [];
    const walk = (up: FakeCard) => {
      for (const c of cards.filter((x) => x.parent === up.number && !found.includes(x))) {
        found.push(c);
        walk(c);
      }
    };
    walk(of);
    return found;
  };
  const isClosed = (c: FakeCard) => c.closed || c.column === "Done";
  /** Whether `blocker` waits, through others, for `target`. */
  const waitsFor = (blocker: FakeCard, target: FakeCard, seen = new Set<number>()): boolean => {
    if (blocker.number === target.number) return true;
    if (seen.has(blocker.number)) return false;
    seen.add(blocker.number);
    return blocker.blocked_by.some((n) => {
      const next = numbered(n);
      return next !== undefined && waitsFor(next, target, seen);
    });
  };
  const ok = (body: unknown) => ({ status: 200, body });
  const fail = (status: number, message: string) => ({ status, body: message });

  /** The routes of the issue tracker; undefined when `path` is none of them. */
  function tracker(method: string, path: string, body: unknown) {
    const parts =
      /^\/v1\/cards\/(\d+)\/(parent|labels|blockers|claim|close|reopen|comments)(?:\/(\d+))?$/.exec(
        path,
      );
    if (!parts) return undefined;
    const target = find(Number(parts[1]));
    if (!target) return fail(404, `card ${parts[1]} not found`);
    const given = (body ?? {}) as Record<string, unknown>;
    const updated = (event: ServerEvent) => send(event);
    switch (parts[2]) {
      case "parent": {
        if (method !== "PUT") return undefined;
        const parent = given.parent === null ? null : Number(given.parent);
        if (parent !== null) {
          const up = numbered(parent);
          if (!up) return fail(404, `issue #${parent} (the parent) not found`);
          if (up.id === target.id) return fail(422, "an issue cannot be its own parent");
          if (below(target).some((c) => c.id === up.id)) {
            return fail(422, `#${parent} is below #${target.number}, so it cannot be its parent`);
          }
        }
        target.parent = parent;
        updated({ kind: "card_parent_changed", data: { card: target.id, parent } });
        return ok(view(target));
      }
      case "labels": {
        if (method !== "PUT") return undefined;
        const names = Array.isArray(given.labels) ? (given.labels as string[]) : [];
        target.labels = [...new Set(names.map((l) => l.trim()).filter((l) => l !== ""))].sort();
        updated({ kind: "card_labels_changed", data: { card: target.id, labels: target.labels } });
        return ok(view(target));
      }
      case "blockers": {
        if (method === "POST") {
          const number = Number(given.blocker);
          const blocker = numbered(number);
          if (!blocker) return fail(404, `issue #${number} (the blocker) not found`);
          if (blocker.id === target.id) return fail(422, "an issue cannot be its own blocker");
          if (waitsFor(blocker, target)) {
            return fail(
              422,
              `#${number} already waits for #${target.number}, so it cannot block it`,
            );
          }
          if (!target.blocked_by.includes(number) && !isClosed(blocker))
            target.blocked_by.push(number);
          updated({
            kind: "card_blocking_changed",
            data: { card: target.id, blocked_by: target.blocked_by },
          });
          return ok(target.blocked_by);
        }
        if (method === "DELETE") {
          const number = Number(parts[3]);
          if (!target.blocked_by.includes(number)) {
            return fail(404, `#${number} among the blockers of #${target.number} not found`);
          }
          target.blocked_by = target.blocked_by.filter((n) => n !== number);
          updated({
            kind: "card_blocking_changed",
            data: { card: target.id, blocked_by: target.blocked_by },
          });
          return ok(target.blocked_by);
        }
        return undefined;
      }
      case "claim": {
        if (method === "POST") {
          if (target.claimed_by !== null && target.claimed_by !== "person") {
            return fail(409, `#${target.number} is claimed by ${target.claimed_by} already`);
          }
          const first = target.claimed_by === null;
          target.claimed_by = "person";
          if (first) updated({ kind: "card_claimed", data: { card: target.id, by: "person" } });
          return ok(view(target));
        }
        if (method === "DELETE") {
          if (target.claimed_by === null) return fail(409, `#${target.number} is not claimed`);
          target.claimed_by = null;
          updated({ kind: "card_released", data: { card: target.id } });
          return ok(view(target));
        }
        return undefined;
      }
      case "close": {
        if (method !== "POST") return undefined;
        if (!target.closed) {
          target.closed = true;
          if (target.column !== "Done") send(move(target, "Done"));
          for (const other of cards)
            other.blocked_by = other.blocked_by.filter((n) => n !== target.number);
          updated({ kind: "card_closed", data: { card: target.id } });
        }
        return ok(view(target));
      }
      case "reopen": {
        if (method !== "POST") return undefined;
        if (!isClosed(target)) return fail(409, `#${target.number} is open`);
        target.closed = false;
        if (target.column === "Done") send(move(target, "Backlog"));
        updated({ kind: "card_reopened", data: { card: target.id } });
        return ok(view(target));
      }
      case "comments": {
        if (method === "GET") return ok(comments.get(target.id) ?? []);
        if (method === "POST") {
          const text = String(given.body ?? "").trim();
          if (text === "") return fail(422, "a comment needs text");
          const made: FakeComment = {
            id: nextComment++,
            card_id: target.id,
            author: "person",
            body: text,
            at: ++clock,
          };
          comments.set(target.id, [...(comments.get(target.id) ?? []), made]);
          updated({ kind: "card_commented", data: { card: target.id, comment: made } });
          return ok(made);
        }
        return undefined;
      }
    }
    return undefined;
  }

  function decide(method: string, path: string, body: unknown) {
    const approve = /^\/v1\/cards\/(\d+)\/approve$/.exec(path);
    const retry = /^\/v1\/cards\/(\d+)\/retry$/.exec(path);
    const merge = /^\/v1\/changes\/(\d+)\/merge$/.exec(path);
    const reject = /^\/v1\/changes\/(\d+)\/reject$/.exec(path);
    const block = /^\/v1\/cards\/(\d+)\/block$/.exec(path);
    const unblock = /^\/v1\/cards\/(\d+)\/unblock$/.exec(path);
    if (method !== "POST") return undefined;
    if (block) {
      const target = find(Number(block[1]));
      if (!target) return fail(404, `card ${block[1]} not found`);
      if (target.held) return fail(409, `card ${target.id} is held already`);
      const reason = ((body as { reason?: string } | undefined)?.reason ?? "").trim();
      target.held = { reason: reason === "" ? null : reason, by: "person", at: ++clock };
      if (target.run && (target.run.status === "running" || target.run.status === "starting")) {
        const stopped = target.run;
        target.run = { ...stopped, status: "stopped" };
        send({ kind: "run_ended", data: { run: stopped.id, status: "stopped" } });
      }
      send({ kind: "card_updated", data: view(target) });
      return ok(view(target));
    }
    const stop = /^\/v1\/runs\/(\d+)\/stop$/.exec(path);
    if (stop) {
      const target = cards.find((c) => c.run?.id === Number(stop[1]));
      const live = target?.run?.status === "running" || target?.run?.status === "starting";
      if (!target?.run || !live) return fail(422, `run ${stop[1]} is not running`);
      const stopped = target.run;
      target.run = { ...stopped, status: "stopped" };
      send({ kind: "run_ended", data: { run: stopped.id, status: "stopped" } });
      send({ kind: "card_updated", data: view(target) });
      return ok(stopped);
    }
    if (unblock) {
      const target = find(Number(unblock[1]));
      if (!target) return fail(404, `card ${unblock[1]} not found`);
      if (!target.held) return fail(409, `card ${target.id} is not held`);
      target.held = null;
      send({ kind: "card_updated", data: view(target) });
      return ok(view(target));
    }
    if (approve) {
      const target = find(Number(approve[1]));
      if (!target) return fail(404, "no such card");
      if (target.column !== "Backlog") return fail(422, "the card is not in Backlog");
      resolve(target.id, ["approve_issue"]);
      target.counts_as_sub_issue = true;
      send(move(target, "Not started"));
      send({ kind: "card_updated", data: view(target) });
      return ok(view(target));
    }
    if (retry) {
      const target = find(Number(retry[1]));
      if (!target) return fail(404, "no such card");
      const open = inbox.some(
        (i) =>
          i.card_id === target.id &&
          i.resolved_at === null &&
          (i.kind === "stuck" || i.kind === "change_closed"),
      );
      if (!open)
        return fail(422, "the card has neither an open stuck nor an open change_closed item");
      resolve(target.id, ["stuck", "change_closed"]);
      target.retried_after = target.run?.id ?? null;
      target.rounds = 0;
      target.tokens = 0;
      target.run = { id: 3, agent: "Coder", purpose: "code", status: "running" };
      send({ kind: "run_started", data: { run: 3, card: target.id } });
      return ok(view(target));
    }
    if (merge) {
      const found = changes.find((c) => c.id === Number(merge[1]));
      if (!found) return fail(404, "no such change");
      if (found.state !== "open") return fail(422, "the change is not open");
      found.state = "merged";
      const target = find(found.card_id);
      if (target) {
        resolve(target.id, ["ready_to_merge"]);
        target.change = null;
        target.rounds = 0;
        target.tokens = 0;
        send(move(target, "Done"));
      }
      send({ kind: "change_merged", data: found });
      return ok({ commit: "5ca1ab1e5ca1ab1e5ca1ab1e5ca1ab1e5ca1ab1" });
    }
    if (reject) {
      const found = changes.find((c) => c.id === Number(reject[1]));
      if (!found) return fail(404, "no such change");
      const target = find(found.card_id);
      if (found.state !== "open" || target?.column !== "Ready to merge") {
        return fail(422, "the change is not open or its card is not in Ready to merge");
      }
      const reason = (body as { reason?: string } | undefined)?.reason;
      target.note = reason || "A person rejected the change.";
      resolve(target.id, ["ready_to_merge"]);
      send(move(target, "In progress", target.note));
      return ok(view(target));
    }
    return undefined;
  }

  return {
    requests,
    snapshot() {
      return {
        boards: [
          {
            repository: repo,
            cards: cards.map(view),
            round_limit: 5,
            waiting: {},
            changes,
          },
        ],
        harness_changes: [],
        inbox: inbox.filter((i) => i.resolved_at === null),
      };
    },
    moveCard(id, column) {
      const target = find(id);
      if (!target) throw new Error(`no card ${id}`);
      return move(target, column);
    },
    refuseNext(method, path, status, message) {
      refusals.set(`${method} ${path}`, { status, message });
    },
    handle(method, path, body) {
      const known =
        path === "/v1/repositories/1/issues" ||
        /^\/v1\/(cards|changes)\/\d+\//.test(path) ||
        /^\/v1\/cards\/\d+$/.test(path) ||
        /^\/v1\/runs\/\d+\/stop$/.test(path) ||
        /^\/v1\/repositories\/1\/scripts\//.test(path);
      if (!known) return undefined;
      if (method !== "GET") requests.push({ method, path, body });
      const refusal = refusals.get(`${method} ${path}`);
      if (refusal) {
        refusals.delete(`${method} ${path}`);
        return fail(refusal.status, refusal.message);
      }
      if (path === "/v1/repositories/1/issues" && method === "POST") {
        const {
          title,
          body: text,
          parent,
        } = (body ?? {}) as { title?: string; body?: string; parent?: number };
        if (!title || title.trim() === "") return fail(422, "the title is empty");
        if (parent !== undefined && !numbered(parent)) {
          return fail(404, `issue #${parent} (the parent) not found`);
        }
        const made = card(nextCard, {
          title: title.trim(),
          body: text ?? "",
          column: "Backlog",
          created_at: ++clock,
          parent: parent ?? null,
        });
        nextCard += 1;
        cards = [...cards, made];
        send({ kind: "card_added", data: view(made) });
        if (parent !== undefined) {
          send({ kind: "card_parent_changed", data: { card: made.id, parent } });
        }
        return ok(view(made));
      }
      const edited = /^\/v1\/cards\/(\d+)$/.exec(path);
      if (edited && method === "PUT") {
        const target = find(Number(edited[1]));
        if (!target) return fail(404, `card ${edited[1]} not found`);
        const { title, body: text } = (body ?? {}) as { title?: string; body?: string };
        if (!title || title.trim() === "") return fail(422, "an issue needs a title");
        target.title = title.trim();
        target.body = text ?? "";
        send({ kind: "card_updated", data: view(target) });
        return ok(view(target));
      }
      const tracked = tracker(method, path, body);
      if (tracked) return tracked;
      const removed = /^\/v1\/cards\/(\d+)$/.exec(path);
      if (removed && method === "DELETE") {
        const target = find(Number(removed[1]));
        if (!target) return fail(404, `card ${removed[1]} not found`);
        cards = cards.filter((c) => c.id !== target.id);
        for (const gone of changes.filter((c) => c.card_id === target.id)) {
          changes.splice(changes.indexOf(gone), 1);
        }
        for (const item of inbox.filter((i) => i.card_id === target.id)) {
          inbox.splice(inbox.indexOf(item), 1);
        }
        history.delete(target.id);
        runs.delete(target.id);
        send({ kind: "card_deleted", data: { card: target.id } });
        return ok(view(target));
      }
      const history$ = /^\/v1\/cards\/(\d+)\/history$/.exec(path);
      if (history$ && method === "GET") {
        const id = Number(history$[1]);
        return find(id) ? ok(history.get(id) ?? []) : fail(404, "no such card");
      }
      const runs$ = /^\/v1\/cards\/(\d+)\/runs$/.exec(path);
      if (runs$ && method === "GET") {
        const id = Number(runs$[1]);
        return find(id) ? ok(runs.get(id) ?? []) : fail(404, "no such card");
      }
      const script = /^\/v1\/repositories\/1\/scripts\/(.+)$/.exec(path);
      if (script && method === "GET") {
        const column = decodeURIComponent(script[1] as string);
        const file = SCRIPTS[column.toLowerCase()];
        if (!file) return fail(404, "no such column");
        return ok({
          column,
          path: `repos/demo/${file}.luau`,
          source: "built-in",
          text: `-- The ${column} column\nmoebius.wait("nothing to do in ${column}")\n`,
        });
      }
      return decide(method, path, body);
    },
  };
}
