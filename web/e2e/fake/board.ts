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
  project_id: number;
  number: number;
  /** Set on the cards that are named by a key; the others are named by their number, as by an older server. */
  project_number?: number;
  project_key?: string;
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
  change: {
    id: number;
    repository: string;
    state: string;
    checks: string;
    review_verdict: string | null;
    outside_author?: string;
    /** The Reviewer's latest round on a followed pull request (ADR 0029). */
    external_review?: {
      round: number;
      round_limit: number;
      state: string;
      verdict: "approve" | "changes" | null;
      summary: string;
      comments: Array<{ path: string; line: number; body: string }>;
      url: string | null;
      head: string;
      current: boolean;
    };
  } | null;
  /** The forge login of someone on another server who took the issue. */
  taken_by?: string | null;
  taken_by_command?: boolean;
  assign_problem?: string | null;
  /** The card's open changes in the repositories besides its own (`seedChange`). */
  more_changes: Array<{
    id: number;
    repository: string;
    state: string;
    checks: string;
    review_verdict: string | null;
  }>;
  /** The repositories the card works in, its own first. */
  repositories: string[];
  /** Set while a person or the Moebius agent holds the card. */
  held: { reason: string | null; by: string; at: number } | null;
  parent: number | null;
  counts_as_sub_issue: boolean;
  /** The issue was closed; a card in Done counts as closed too (see `view`). */
  closed: boolean;
  labels: string[];
  claimed_by: string | null;
  assignees: string[];
  priority: string;
  due: string | null;
  estimate: number | null;
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
  outside_author?: string;
  taken_from?: string;
}

interface FakeInboxItem {
  id: number;
  kind: string;
  card_id: number | null;
  title: string;
  data: Record<string, unknown>;
  resolved_at: number | null;
  assignees: string[];
  at: number;
}

interface HistoryEntry {
  at: number;
  kind: string;
  data: Record<string, unknown>;
}

/** A title longer than any column: one word with no break, and then words. */
export const LONG_TITLE = `${"Supercalifragilistic".repeat(8)} and then a title that goes on for a long while so that it wraps`;

/**
 * The columns of the project, as the server sends them: the six built-in ones and the two a `kanban.toml` adds after
 * In review, which count as In review. `file` is the script of the column.
 */
const BUILT_IN: string[] = [
  "Backlog",
  "Not started",
  "In progress",
  "In review",
  "Ready to merge",
  "Done",
];

export const COLUMNS = [
  { name: "Backlog", counts_as: "Backlog", file: "backlog" },
  { name: "Not started", counts_as: "Not started", file: "ready" },
  { name: "In progress", counts_as: "In progress", file: "code" },
  { name: "In review", counts_as: "In review", file: "review" },
  { name: "Ready for testing", counts_as: "In review", file: "testing" },
  { name: "Being measured", counts_as: "In review", file: "measured" },
  { name: "Ready to merge", counts_as: "Ready to merge", file: "merge" },
  { name: "Done", counts_as: "Done", file: "done" },
] as const;

const SCRIPTS: Record<string, string> = {
  "ready for testing": "testing",
  "being measured": "measured",
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
    home_project_id: 1,
  };
}

function card(id: number, extra: Partial<FakeCard>): FakeCard {
  return {
    id,
    repository_id: 1,
    project_id: 1,
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
    more_changes: [],
    repositories: ["demo"],
    held: null,
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: [],
    claimed_by: null,
    assignees: [],
    priority: "none",
    due: null,
    estimate: null,
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
  /** The issue template of a kind, which `use_template` fills a new issue from; set by the templates fake. */
  issueTemplate: (kind: string) => { title: string; body: string } | undefined;
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
  /**
   * Puts `count` more cards in the Backlog that wait for a person's approval, as an outside author's issues do, and
   * returns their numbers. The page reads the board again on the next event.
   */
  seedApprovals(count: number): number[];
  /**
   * Puts three cards in the project's custom columns, two in "Ready for testing" and one in "Being measured", numbered
   * after the last, and returns their numbers. The columns themselves are always there.
   */
  seedCustomColumns(): number[];
  /** Adds `count` plain cards to `column`, numbered after the last, and returns their numbers in order. */
  seedCards(count: number, column: string): number[];
  /**
   * Gives card `id` an open change in a second repository `web`, which the card now works in: a failing one, as the
   * checks of that repository are its own.
   */
  seedChange(id: number): void;
  /**
   * Puts two cards of other people's work on the board: an issue `alice` took on another server (Not started), and a
   * card in review that follows her pull request 7, with its inbox item. Returns their numbers.
   */
  seedElsewhere(): { taken: number; followed: number };
  /**
   * Puts two issues `alice` took with `/take` in a comment on the board (Not started): one just taken, and one she says
   * is done, which waits for the owner with its inbox item. Returns their numbers.
   */
  seedCommands(): { takenWith: number; asked: number };
  /**
   * Puts the owner's daily digest (ADR 0031) in the inbox: what merged, was refused, reviewed, taken and handled as stale
   * in the demo repository, the tokens against the budget, and what needs a person. Returns the item's id.
   */
  seedDigest(): number;
  /** Makes the next request with `method` to `path` fail with `status` and `message`. */
  refuseNext(method: string, path: string, status: number, message: string): void;
  /** The non-GET requests the board routes received, in order. */
  requests: Array<{ method: string; path: string; body: unknown }>;
}

export function createBoardFake(send: (event: ServerEvent) => void): BoardFake {
  const repo: Record<string, unknown> = repository();
  let logins = { github: "", gitlab: "", forgejo: "" };
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
      body: "Keep it small: cache the compiled assets and fail the build when the cache is stale.",
      column: "Ready to merge",
      rounds: 2,
      tokens: 96_000,
      change: {
        id: 1,
        repository: "demo",
        state: "open",
        checks: "success",
        review_verdict: "approve",
      },
    }),
    card(6, {
      title: "Stop the flaky test",
      column: "In progress",
      rounds: 5,
      tokens: 480_000,
      run: { id: 2, agent: "Reviewer", purpose: "review", status: "failed" },
    }),
    card(7, {
      title: "Ship the first release",
      column: "Done",
      project_key: "DEMO",
      project_number: 31,
    }),
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
      assignees: [],
      at: NOW - 100,
    },
    {
      id: 2,
      kind: "ready_to_merge",
      card_id: 5,
      title: "Speed up the build is ready to merge",
      data: { change: 1, branch: "moebius/card-5" },
      resolved_at: null,
      assignees: [],
      at: NOW - 90,
    },
    {
      id: 3,
      kind: "stuck",
      card_id: 6,
      title: "Stop the flaky test is stuck",
      data: { reason: "reached the round limit" },
      resolved_at: null,
      assignees: [],
      at: NOW - 80,
    },
  ];
  const history = new Map<number, HistoryEntry[]>([
    [
      5,
      [
        {
          at: NOW - 3000,
          kind: "moved",
          data: { from: "Not started", to: "In progress", note: null },
        },
        { at: NOW - 2900, kind: "run_started", data: { run: 11, agent: "Coder", purpose: "code" } },
        { at: NOW - 2600, kind: "run_ended", data: { run: 11, status: "done" } },
        {
          at: NOW - 2500,
          kind: "run_started",
          data: { run: 12, agent: "Reviewer", purpose: "review" },
        },
        { at: NOW - 2400, kind: "run_ended", data: { run: 12, status: "done" } },
      ],
    ],
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
  const view = (c: FakeCard) => {
    const { more_changes, ...rest } = c;
    return {
      ...rest,
      closed: c.closed || c.column === "Done",
      column_class: COLUMNS.find((each) => each.name === c.column)?.counts_as ?? "Not started",
      changes: [...(c.change ? [c.change] : []), ...more_changes],
    };
  };
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
      /^\/v1\/cards\/(\d+)\/(parent|labels|assignees|priority|due|estimate|blockers|claim|close|reopen|comments)(?:\/([^/]+))?$/.exec(
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
      case "priority":
      case "due": {
        if (method !== "PUT") return undefined;
        // Both are labels of the card, as on the server.
        const field = parts[2];
        const value = given[field];
        if (field === "priority") {
          const names = ["urgent", "high", "medium", "low", "none"];
          if (typeof value !== "string" || !names.includes(value.toLowerCase())) {
            return fail(422, `${JSON.stringify(value)} is not a priority`);
          }
          target.priority = value.toLowerCase();
        } else {
          if (value !== null && !(typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value))) {
            return fail(422, `${JSON.stringify(value)} is not a date (YYYY-MM-DD)`);
          }
          target.due = value;
        }
        target.labels = [
          ...target.labels.filter((l) => !l.startsWith("priority:") && !l.startsWith("due:")),
          ...(target.priority === "none" ? [] : [`priority:${target.priority}`]),
          ...(target.due === null ? [] : [`due:${target.due}`]),
        ].sort();
        updated({ kind: "card_labels_changed", data: { card: target.id, labels: target.labels } });
        return ok(view(target));
      }
      case "estimate": {
        if (method !== "PUT") return undefined;
        const value = given.estimate;
        if (
          value !== null &&
          !(Number.isInteger(value) && Number(value) >= 1 && Number(value) <= 1000)
        ) {
          return fail(422, `${JSON.stringify(value)} is not an estimate (1 to 1000 points)`);
        }
        target.estimate = value as number | null;
        target.labels = [
          ...target.labels.filter((l) => !l.startsWith("estimate:")),
          ...(target.estimate === null ? [] : [`estimate:${target.estimate}`]),
        ].sort();
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
      case "assignees": {
        if (parts[3] !== undefined) {
          // One person added or taken off, applied to the people the card has now.
          if (method !== "PUT" && method !== "DELETE") return undefined;
          const handle = decodeURIComponent(parts[3]).replace(/^@/, "").toLowerCase();
          const has = target.assignees.includes(handle);
          if (method === "PUT" && !has) target.assignees = [...target.assignees, handle];
          if (method === "DELETE" && has)
            target.assignees = target.assignees.filter((h) => h !== handle);
          if (has !== (method === "PUT")) {
            updated({
              kind: "card_assignees_changed",
              data: { card: target.id, assignees: target.assignees },
            });
          }
          return ok(view(target));
        }
        if (method !== "PUT") return undefined;
        const handles = (body as { assignees?: unknown }).assignees;
        if (!Array.isArray(handles) || !handles.every((h) => typeof h === "string")) {
          return fail(422, "assignees are handles");
        }
        const next = [...new Set(handles.map((h) => h.replace(/^@/, "").toLowerCase()))];
        if (next.join() !== target.assignees.join()) {
          target.assignees = next;
          updated({
            kind: "card_assignees_changed",
            data: { card: target.id, assignees: target.assignees },
          });
        }
        return ok(view(target));
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
    const review = /^\/v1\/changes\/(\d+)\/review$/.exec(path);
    const done = /^\/v1\/cards\/(\d+)\/done$/.exec(path);
    const releaseTaken = /^\/v1\/cards\/(\d+)\/release-taken$/.exec(path);
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
    if (done || releaseTaken) {
      const target = find(Number((done ?? releaseTaken)?.[1]));
      if (!target) return fail(404, "no such card");
      const asked = inbox.some(
        (i) => i.card_id === target.id && i.kind === "done_asked" && i.resolved_at === null,
      );
      const answer = (body as { answer?: string } | null)?.answer;
      if (done && !asked)
        return fail(409, "nobody has said this issue is done, or it was answered");
      if (done && !["close", "give_back", "release"].includes(answer ?? ""))
        return fail(422, `answer is "close", "give_back" or "release", not ${answer}`);
      if (releaseTaken && !target.taken_by_command)
        return fail(409, `#${target.number} was not taken with /take`);
      resolve(target.id, ["done_asked"]);
      if (releaseTaken || answer === "release") {
        target.taken_by = null;
        target.taken_by_command = false;
      }
      if (answer === "close") send(move(target, "Done"));
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
        resolve(target.id, ["ready_to_merge", "outside_change"]);
        target.change = null;
        target.rounds = 0;
        target.tokens = 0;
        send(move(target, "Done"));
      }
      send({ kind: "change_merged", data: found });
      return ok({ commit: "5ca1ab1e5ca1ab1e5ca1ab1e5ca1ab1e5ca1ab1" });
    }
    if (review) {
      const found = changes.find((c) => c.id === Number(review[1]));
      if (!found?.outside_author) return fail(422, "not a pull request someone else opened");
      return ok({
        id: 50,
        card_id: found.card_id,
        agent: "Reviewer",
        purpose: "review",
        status: "running",
      });
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

  const fake: BoardFake = {
    issueTemplate: () => undefined,
    requests,
    snapshot() {
      return {
        boards: [
          {
            repository: repo,
            project_columns: {
              "1": COLUMNS.map(({ name, counts_as, file }) => ({
                name,
                counts_as,
                custom: !BUILT_IN.includes(name),
                script: `${file}.luau`,
              })),
            },
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
    seedCustomColumns() {
      const first = Math.max(...cards.map((c) => c.id)) + 1;
      const titles: Array<[string, string]> = [
        ["Try the export by hand", "Ready for testing"],
        ["Check the cache hit rate", "Ready for testing"],
        ["Measure the cold start", "Being measured"],
      ];
      return titles.map(([title, column], at) => {
        cards.push(card(first + at, { title, column }));
        return first + at;
      });
    },
    seedCards(count, column) {
      const first = Math.max(...cards.map((c) => c.id)) + 1;
      const made = Array.from({ length: count }, (_, at) => first + at);
      for (const id of made) cards.push(card(id, { title: `Seeded card ${id}`, column }));
      return made;
    },
    seedApprovals(count) {
      const made: number[] = [];
      for (let i = 0; i < count; i++) {
        const id = Math.max(...cards.map((c) => c.id)) + 1;
        cards.push(
          card(id, { title: `Outside issue ${id}`, column: "Backlog", author: "stranger" }),
        );
        inbox.push({
          id: Math.max(...inbox.map((item) => item.id)) + 1,
          kind: "approve_issue",
          card_id: id,
          title: `Approve: Outside issue ${id}`,
          data: { reason: "An outside author opened it" },
          resolved_at: null,
          assignees: [],
          at: NOW - 70,
        });
        made.push(id);
      }
      return made;
    },
    seedChange(id) {
      const target = find(id);
      if (!target) throw new Error(`no card ${id}`);
      if (!target.repositories.includes("web")) target.repositories.push("web");
      target.more_changes = [
        { id: 20, repository: "web", state: "open", checks: "failure", review_verdict: "approve" },
      ];
      send({ kind: "card_updated", data: view(target) });
    },
    seedElsewhere() {
      const first = Math.max(...cards.map((c) => c.id)) + 1;
      cards.push(
        card(first, {
          title: "Add dark mode to the settings page",
          column: "Not started",
          taken_by: "alice",
        }),
        card(first + 2, {
          title: "Tidy the changelog",
          column: "Not started",
          assignees: ["lea"],
        }),
        card(first + 1, {
          title: "Cache the compiled assets",
          column: "In review",
          taken_by: "alice",
          change: {
            id: 30,
            repository: "demo",
            state: "open",
            checks: "success",
            review_verdict: "changes",
            outside_author: "alice",
            external_review: {
              round: 1,
              round_limit: 3,
              state: "posted",
              verdict: "changes",
              summary: "The cache is never emptied, and nothing tests the new path.",
              comments: [
                {
                  path: "src/cache.rs",
                  line: 42,
                  body: "This grows without a bound: evict the oldest entry when the map is full.",
                },
                {
                  path: "src/cache.rs",
                  line: 57,
                  body: "Add a test that reads a cached asset twice.",
                },
              ],
              url: "https://github.test/acme/demo/pull/7#pullrequestreview-91",
              head: "abc",
              // The author pushed since: the review is of an earlier version, so the owner may merge.
              current: false,
            },
          },
        }),
      );
      changes.push(
        change(30, first + 1, {
          pull_request: 7,
          branch: "moebius/pr-7",
          outside_author: "alice",
          taken_from: "In progress",
        }),
      );
      inbox.push({
        id: Math.max(...inbox.map((item) => item.id)) + 1,
        kind: "outside_change",
        card_id: first + 1,
        title: "Pull request #7 by @alice closes this issue",
        data: { change: 30, pull_request: 7, author: "alice", repository: "demo" },
        resolved_at: null,
        assignees: [],
        at: NOW - 60,
      });
      return { taken: first, followed: first + 1 };
    },
    seedCommands() {
      const first = Math.max(...cards.map((c) => c.id)) + 1;
      cards.push(
        card(first, {
          title: "Add dark mode to the settings page",
          column: "Not started",
          taken_by: "alice",
          taken_by_command: true,
        }),
        card(first + 1, {
          title: "Cache the compiled assets",
          column: "Not started",
          taken_by: "alice",
          taken_by_command: true,
        }),
      );
      inbox.push({
        id: Math.max(...inbox.map((item) => item.id)) + 1,
        kind: "done_asked",
        card_id: first + 1,
        title: "@alice says #" + (first + 1) + " is done: Cache the compiled assets",
        data: {
          login: "alice",
          reason: "The assets are cached now.\nSee the notes in the description.",
          repository: "demo",
        },
        resolved_at: null,
        assignees: [],
        at: NOW - 90,
      });
      return { takenWith: first, asked: first + 1 };
    },
    seedDigest() {
      const id = Math.max(...inbox.map((item) => item.id)) + 1;
      const line = (
        number: number | null,
        title: string,
        who: string | null,
        detail = "",
        measured = false,
      ) => ({ number, title, who, detail, measured });
      inbox.push({
        id,
        kind: "daily_digest",
        card_id: null,
        title: "Daily digest 2026-11-25: 3 merged, 2 need you",
        data: {
          day: "2026-11-25",
          since: NOW - 86_400,
          until: NOW,
          repositories: [
            {
              id: 1,
              name: "demo",
              merged: [
                line(24, "Cache the compiled plan", "moebius", "", true),
                line(25, "Fix the login redirect", "alice"),
                line(26, "Shorter prompts for the Reviewer", "moebius"),
              ],
              refused: [
                line(27, "Speed up the build", null, "the checks of pull request 31 failed"),
              ],
              reverts: [
                line(22, "Retry the fetch twice", null, "it doubled the load on the forge"),
              ],
              review_runs: 3,
              rounds: [
                line(31, "", null, "round 1: changes requested"),
                line(31, "", null, "round 2: approved"),
              ],
              taken: [line(18, "Document the API", "bob"), line(19, "Add dark mode", "carol")],
              released: [line(14, "Rework the importer", "dave")],
              upkeep: [
                line(
                  14,
                  "Rework the importer",
                  "dave",
                  "released: taken 15 days ago, no pull request followed",
                ),
                line(
                  33,
                  "Rota page",
                  "erin",
                  "reminded: no push for 14 days after a review asked for changes",
                ),
              ],
              frozen: [line(8, "Migrate the settings", "person", "waiting for the release")],
              more: {},
            },
          ],
          tokens: { spent: 412_300, day_budget: 1_000_000, today: 98_000 },
          needs_a_person: [
            {
              kind: "ready_to_merge",
              reason: "Ready to merge",
              items: [
                { id: 2, title: "Speed up the build", card_id: 5, number: 5, repository: "demo" },
              ],
            },
            {
              kind: "approve_issue",
              reason: "Outside issue",
              items: [
                {
                  id: 1,
                  title: "Fix the login redirect",
                  card_id: 2,
                  number: 2,
                  repository: "demo",
                },
              ],
            },
          ],
          text: "",
        },
        resolved_at: null,
        assignees: [],
        at: NOW - 30,
      });
      return id;
    },
    refuseNext(method, path, status, message) {
      refusals.set(`${method} ${path}`, { status, message });
    },
    handle(method, path, body) {
      if (path === "/v1/logins") {
        if (method === "PUT") {
          requests.push({ method, path, body });
          logins = { github: "", gitlab: "", forgejo: "", ...(body as object) };
        }
        return ok(logins);
      }
      const dismissed = /^\/v1\/digest\/(\d+)\/dismiss$/.exec(path);
      if (dismissed && method === "POST") {
        requests.push({ method, path, body });
        const item = inbox.find((i) => i.id === Number(dismissed[1]) && i.kind === "daily_digest");
        if (!item) return fail(404, "no open digest");
        item.resolved_at = ++clock;
        return ok({});
      }
      if (path === "/v1/repositories/1" && method === "PUT") {
        requests.push({ method, path, body });
        Object.assign(repo, body);
        send({ kind: "repository_updated", data: repo });
        return ok(repo);
      }
      const known =
        path === "/v1/repositories/1/issues" ||
        /^\/v1\/(cards|changes)\/\d+\//.test(path) ||
        /^\/v1\/cards\/\d+$/.test(path) ||
        /^\/v1\/cards\/by-key\/[^/]+$/.test(path) ||
        /^\/v1\/runs\/\d+\/stop$/.test(path) ||
        /^\/v1\/projects\/1\/scripts\//.test(path);
      if (!known) return undefined;
      if (method !== "GET") requests.push({ method, path, body });
      const refusal = refusals.get(`${method} ${path}`);
      if (refusal) {
        refusals.delete(`${method} ${path}`);
        return fail(refusal.status, refusal.message);
      }
      const byKey = /^\/v1\/cards\/by-key\/([^/]+)$/.exec(path);
      if (byKey && method === "GET") {
        const wanted = decodeURIComponent(byKey[1] ?? "").toUpperCase();
        const found = cards.find((c) => `${c.project_key}-${c.project_number}` === wanted);
        return found ? ok(view(found)) : fail(404, `card ${wanted} not found`);
      }
      if (path === "/v1/repositories/1/issues" && method === "POST") {
        const {
          title: given,
          body: givenText,
          parent,
          kind,
          use_template: useTemplate,
        } = (body ?? {}) as {
          title?: string;
          body?: string;
          parent?: number;
          kind?: string;
          use_template?: boolean;
        };
        let title = given;
        let text = givenText;
        if (useTemplate) {
          if (!kind) return fail(422, "use_template needs a kind");
          const template = fake.issueTemplate(kind);
          if (!template) return fail(404, `the kind ${kind} has no template`);
          if (!title || title.trim() === "") title = template.title;
          if (!text || text.trim() === "") text = template.body;
        }
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
      const named = /^\/v1\/cards\/(\d+)\/repositories$/.exec(path);
      if (named && method === "PUT") {
        const target = find(Number(named[1]));
        if (!target) return fail(404, `card ${named[1]} not found`);
        const wanted = (((body ?? {}) as { repositories?: string[] }).repositories ?? []).filter(
          (name) => name !== "demo",
        );
        // The project of the card has one repository besides its own: `web`.
        const unknown = wanted.find((name) => name !== "web");
        if (unknown) return fail(422, `${unknown} is not a repository of the project demo`);
        if (!wanted.includes("web") && target.more_changes.some((c) => c.repository === "web")) {
          return fail(
            422,
            "the card has an open change in web: it cannot leave the card while the change is open",
          );
        }
        target.repositories = ["demo", ...wanted];
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
      const script = /^\/v1\/projects\/1\/scripts\/(.+)$/.exec(path);
      if (script && method === "GET") {
        const column = decodeURIComponent(script[1] as string);
        const file = SCRIPTS[column.toLowerCase()];
        if (!file) return fail(404, "no such column");
        return ok({
          column,
          path: `projects/demo/${file}.luau`,
          source: "built-in",
          text: `-- The ${column} column\nmoebius.wait("nothing to do in ${column}")\n`,
        });
      }
      return decide(method, path, body);
    },
  };
  return fake;
}
