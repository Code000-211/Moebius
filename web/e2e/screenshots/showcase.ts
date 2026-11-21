/**
 * The data of the README's pictures: one project, `storefront`, with cards in every column, three agents at work, the
 * inbox, and the run events and the diff the pages read. The fake server of `../fakeServer.ts` answers everything
 * else; `readme.test.ts` puts these answers in front of it, with times counted back from now so that nothing reads
 * as weeks old.
 */

const now = () => Math.floor(Date.now() / 1000);
const ago = (seconds: number) => now() - seconds;

/** One event of a run, as `GET /v1/runs/{id}/events` lists it. */
export interface RunEvent {
  seq: number;
  kind: string;
  data: Record<string, unknown>;
  at: number;
}

/** One step of an agent: some words, or a tool call that has an output, or one still running (no output). */
export type Step =
  | { say: string }
  | { think: string }
  | { tool: string; args: Record<string, unknown>; output?: string };

/** The events Pi-Bolt and the server make for these steps, the last one ending `lastAt` seconds ago. */
export function events(steps: Step[], lastAt = 5): RunEvent[] {
  const list: RunEvent[] = [];
  const push = (kind: string, data: Record<string, unknown>) =>
    list.push({ seq: list.length + 1, kind, data, at: ago(lastAt + (steps.length - list.length)) });
  push("agent_start", { type: "agent_start" });
  steps.forEach((step, index) => {
    if ("think" in step) {
      push("message_update", {
        type: "message_update",
        assistantMessageEvent: { type: "thinking_delta", contentIndex: 0, delta: step.think },
      });
      return;
    }
    if ("say" in step) {
      push("message_update", {
        type: "message_update",
        assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: step.say },
      });
      return;
    }
    const id = `call-${index}`;
    push("tool_check", {
      tool: step.tool,
      tool_call_id: id,
      decision: "allow",
      reason: "no rule is broken",
    });
    push("tool_execution_start", {
      type: "tool_execution_start",
      toolCallId: id,
      toolName: step.tool,
      args: step.args,
    });
    if (step.output !== undefined) {
      push("tool_execution_end", {
        type: "tool_execution_end",
        toolCallId: id,
        toolName: step.tool,
        isError: false,
        durationMs: 840,
        result: { content: [{ type: "text", text: step.output }] },
      });
    }
  });
  return list;
}

const ROOT = "/srv/.moebius/runs";

export const repository = {
  id: 1,
  name: "storefront",
  path: "/srv/.moebius/repos/storefront",
  forge: "github",
  default_branch: "main",
  github: "acme/storefront",
  trusted: ["maya"],
  home_project_id: 1,
};

/** The steps of the runs the Fleet and the board show. */
export const runSteps: Record<number, Step[]> = {
  101: [
    {
      say: "The discount is rounded per line, so a cart of three items drifts by a cent. I will round once, on the total.",
    },
    {
      tool: "read",
      args: { path: `${ROOT}/101/worktree/src/cart/discount.rs` },
      output: "pub fn apply(cart: &Cart, code: &Code) -> Money {",
    },
    {
      tool: "edit",
      args: { path: `${ROOT}/101/worktree/src/cart/discount.rs` },
      output: "Edited src/cart/discount.rs (+9 -6)",
    },
    {
      tool: "bash",
      args: { command: "cargo test -p cart discount" },
      output: "test result: FAILED. 11 passed; 1 failed",
    },
    {
      tool: "read",
      args: { path: `${ROOT}/101/worktree/tests/discount.rs` },
      output: "#[test] fn three_items_round_once() {",
    },
    {
      tool: "edit",
      args: { path: `${ROOT}/101/worktree/src/cart/discount.rs` },
      output: "Edited src/cart/discount.rs (+3 -1)",
    },
    { tool: "bash", args: { command: "cargo test -p cart discount" } },
  ],
  102: [
    {
      tool: "bash",
      args: { command: 'rg -n "fn deliver" src' },
      output: "src/webhooks/deliver.rs:41: pub async fn deliver(",
    },
    {
      tool: "read",
      args: { path: `${ROOT}/102/worktree/src/webhooks/deliver.rs` },
      output: "pub async fn deliver(hook: &Hook, body: Bytes) -> Result<()> {",
    },
    {
      say: "Deliveries are tried once. I will retry up to five times with exponential backoff and jitter, and keep the failures in the table.",
    },
    {
      tool: "write",
      args: { path: `${ROOT}/102/worktree/src/webhooks/backoff.rs` },
      output: "Wrote src/webhooks/backoff.rs (38 lines)",
    },
    {
      tool: "edit",
      args: { path: `${ROOT}/102/worktree/src/webhooks/deliver.rs` },
      output: "Edited src/webhooks/deliver.rs (+21 -4)",
    },
    {
      tool: "bash",
      args: { command: "cargo clippy -p webhooks -- -D warnings" },
      output: "Finished dev profile",
    },
  ],
  103: [
    {
      tool: "bash",
      args: { command: "git diff main...HEAD --stat" },
      output: "4 files changed, 96 insertions(+), 12 deletions(-)",
    },
    {
      tool: "read",
      args: { path: `${ROOT}/103/worktree/src/checkout/address.rs` },
      output: "pub fn validate(address: &Address) -> Result<(), AddressError> {",
    },
    {
      say: "The postcode check covers the UK and the US only. The issue asks for every country we ship to.",
    },
    {
      tool: "bash",
      args: { command: "rg -n SHIPPING_COUNTRIES src" },
      output: "src/checkout/countries.rs:3: pub const SHIPPING_COUNTRIES: [&str; 31]",
    },
  ],
};

function card(id: number, extra: Record<string, unknown>) {
  return {
    id,
    repository_id: 1,
    project_id: 1,
    project_key: "SHOP",
    project_number: id,
    number: id,
    title: "",
    body: "",
    author: "maya",
    column: "Backlog",
    created_at: ago(86_400 * 3),
    note: null,
    retried_after: null,
    confirmed: false,
    rounds: 0,
    tokens: 0,
    run: null,
    blocked_by: [] as number[],
    refusals: 0,
    change: null,
    held: null,
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: [] as string[],
    claimed_by: null,
    assignees: [],
    priority: "none",
    due: null,
    estimate: null,
    ...extra,
  };
}

const change = {
  id: 1,
  repository_id: 1,
  card_id: 19,
  branch: "moebius/card-19",
  state: "open",
  pull_request: 214,
  checks: "success",
  review: "approve",
  head: "9f3a1c7d2b4e",
  published_at: ago(1_500),
  checks_head: "9f3a1c7d2b4e",
  failed_checks: null,
  check_output: null,
};

const change2 = {
  ...change,
  id: 2,
  card_id: 21,
  branch: "moebius/card-21",
  pull_request: 218,
  review: "changes",
  head: "41d0be77c9a2",
  published_at: ago(5_000),
  checks_head: "41d0be77c9a2",
};

export const snapshot = () => ({
  boards: [
    {
      repository,
      cards: [
        card(31, { title: "Add pagination to the orders endpoint", author: "maya" }),
        card(34, {
          title: "Support Apple Pay at checkout",
          author: "kfrost",
          created_at: ago(7_200),
        }),
        card(28, { title: "Return 404 instead of 500 for an unknown SKU", column: "Not started" }),
        card(29, { title: "Cache the product catalogue for 60 seconds", column: "Not started" }),
        card(27, {
          title: "Send the order confirmation email",
          column: "Not started",
          held: { reason: "Waiting for the mail provider's API key", by: "person", at: ago(5_400) },
        }),
        card(30, {
          title: "Add the refunds endpoint",
          column: "Not started",
          blocked_by: [24],
        }),
        card(24, {
          title: "Fix the rounding of discounts in the cart",
          column: "In progress",
          rounds: 1,
          tokens: 41_000,
          run: { id: 101, agent: "Coder", purpose: "code", status: "running" },
        }),
        card(26, {
          title: "Retry failed webhook deliveries with backoff",
          column: "In progress",
          rounds: 2,
          tokens: 118_000,
          run: { id: 102, agent: "Coder", purpose: "code", status: "running" },
        }),
        card(22, {
          title: "Validate the shipping address before payment",
          column: "In review",
          rounds: 1,
          tokens: 67_000,
          run: { id: 103, agent: "Reviewer", purpose: "review", status: "running" },
        }),
        card(21, {
          title: "A stale cart save overwrites a newer cart",
          column: "In review",
          assignees: ["maya"],
          rounds: 1,
          tokens: 52_000,
          created_at: ago(3 * 3_600),
          change: { id: 2, state: "open", checks: "success", review_verdict: "changes" },
        }),
        card(19, {
          title: "Add an index on orders.customer_id",
          column: "Ready to merge",
          rounds: 1,
          tokens: 38_000,
          change: { id: 1, state: "open", checks: "success", review_verdict: "approve" },
        }),
        card(17, { title: "Upgrade axum to 0.8", column: "Done", created_at: ago(86_400 * 6) }),
        card(18, {
          title: "Log the request id on every error",
          column: "Done",
          created_at: ago(86_400 * 6),
        }),
      ],
      round_limit: 5,
      waiting: { "30": "waiting on #24" },
      changes: [change, change2],
    },
  ],
  harness_changes: [],
  inbox: [
    {
      id: 1,
      kind: "ready_to_merge",
      card_id: 19,
      title: "Add an index on orders.customer_id is ready to merge",
      data: { change: 1, branch: "moebius/card-19" },
      resolved_at: null,
      assignees: [],
      at: ago(1_400),
    },
    {
      id: 2,
      kind: "approve_issue",
      card_id: 34,
      title: "Approve: Support Apple Pay at checkout",
      data: { reason: "An outside author opened it" },
      resolved_at: null,
      assignees: [],
      at: ago(7_100),
    },
    {
      id: 3,
      kind: "proposal",
      card_id: null,
      title: "Tell the Coder to run the tests first",
      data: {
        proposal: 1,
        branch: "proposal/2026-11-04-run-tests-first",
        trials: [
          { card: 24, outcome: "positive", reason: "It passed the checks in one round." },
          {
            card: 26,
            outcome: "positive",
            reason: "It found its own failing test before the review.",
          },
        ],
        regression: [{ card: 18, outcome: "positive", reason: "Still passes." }],
      },
      resolved_at: null,
      assignees: [],
      at: ago(40_000),
    },
    {
      id: 4,
      kind: "approve_issue",
      card_id: 21,
      title:
        "The review found a data loss: a save built on an older cart replaces the newer one. I propose a check that rejects stale saves and shows the current cart, plus a test.",
      data: { reason: "The review asked for a change" },
      resolved_at: null,
      assignees: [],
      at: ago(120),
    },
  ],
});

/** What `GET /v1/cards/21/comments` and `/history` answer: a comment, a move, a code run and a review. */
export const threadOf21 = {
  comments: [
    {
      id: 1,
      card_id: 21,
      author: "maya",
      body: "Keep it small: send the cart version back with the save and fail when it is stale.",
      at: ago(3 * 3_600),
    },
  ],
  history: [
    { at: ago(3 * 3_600 - 60), kind: "moved", data: { from: "Not started", to: "In progress" } },
    {
      at: ago(10_200),
      kind: "run_started",
      data: { run: 111, agent: "Coder", purpose: "code" },
    },
    { at: ago(8_400), kind: "run_ended", data: { run: 111, status: "done" } },
    { at: ago(8_300), kind: "moved", data: { from: "In progress", to: "In review" } },
    {
      at: ago(8_200),
      kind: "run_started",
      data: { run: 112, agent: "Reviewer", purpose: "review" },
    },
    { at: ago(7_100), kind: "run_ended", data: { run: 112, status: "done" } },
  ],
};

/** A diff of 84 added and 12 removed lines, for the change of card 21. */
export const diff21 = (() => {
  const added = Array.from({ length: 84 }, (_, i) => `+    // line ${i + 1} of the version check`);
  const removed = Array.from({ length: 12 }, (_, i) => `-    let old_${i} = save(cart);`);
  return `diff --git a/src/cart/save.rs b/src/cart/save.rs
index 8d21f0a..c5e7b13 100644
--- a/src/cart/save.rs
+++ b/src/cart/save.rs
@@ -1,12 +1,84 @@
${removed.join("\n")}
${added.join("\n")}
`;
})();

export const diff = `diff --git a/migrations/0042_orders_customer_index.sql b/migrations/0042_orders_customer_index.sql
new file mode 100644
index 0000000..4b1c2d9
--- /dev/null
+++ b/migrations/0042_orders_customer_index.sql
@@ -0,0 +1,3 @@
+-- Listing a customer's orders scanned the whole table.
+CREATE INDEX CONCURRENTLY IF NOT EXISTS orders_customer_id_idx
+    ON orders (customer_id, created_at DESC);
diff --git a/src/orders/repo.rs b/src/orders/repo.rs
index 8d21f0a..c5e7b13 100644
--- a/src/orders/repo.rs
+++ b/src/orders/repo.rs
@@ -31,9 +31,10 @@ impl OrderRepo {
     pub async fn for_customer(&self, customer: CustomerId) -> Result<Vec<Order>> {
         let rows = sqlx::query_as!(
             OrderRow,
-            "SELECT * FROM orders WHERE customer_id = $1",
+            "SELECT * FROM orders WHERE customer_id = $1 ORDER BY created_at DESC",
             customer.0
         )
         .fetch_all(&self.pool)
         .await?;
         Ok(rows.into_iter().map(Order::from).collect())
     }
`;

/** The question put to the Moebius agent and the steps of its answer: a thought, two reads, a code block and a plan. */
export const agentQuestion = "Why did card #21 stop?";
export const agentAnswer = `A save built on an older cart replaces the newer one. I can add a version check:

\`\`\`rust path=cart/save.rs
if base != current.version {
    return stale(current); // 409
}
\`\`\`

Shall I go ahead?`;
export const agentSteps: Step[] = [
  {
    think: "The review of card #21 found a data loss. I want to see the save code and the review.",
  },
  {
    tool: "read",
    args: { path: `${ROOT}/111/worktree/src/cart/save.rs` },
    output: "pub async fn save(cart: Cart) -> Result<()> {",
  },
  {
    tool: "read",
    args: { path: `${ROOT}/112/review.md` },
    output: "The save has no version check.",
  },
  { say: agentAnswer },
];
