/**
 * The design audit. It opens every page of the built app in Chromium, in both colour themes at both widths, measures
 * what [`docs/design-web.md`](../../docs/design-web.md) asks for, and writes a picture and a report per page under the folder named by
 * MOEBIUS_DESIGN_OUT: `<label>/<theme>-<width>-<page>.png`, `report.json` with every finding, and `report.md` with the
 * counts and the first findings of each kind.
 *
 * What it measures, per page: sideways overflow and who causes it; text clipped by its own box, where a line that
 * ends in an ellipsis is saying there is more rather than being cut; border radii outside
 * the scale (6, 8, 12); padding, margin and gap that are not on the grid; type sizes and weights outside the scale
 * (11 to 20); interactive rows under the 28px of the quiet standard's smallest row; the contrast of text against what
 * is really behind it; children of a centred row whose middle is
 * not in line; a raised surface inside another raised surface, which stacks one elevation on another; and a sheet
 * whose last thing stops short of its own bottom padding, which leaves a band of nothing under it; a coloured pill that
 * stands for a state (`pills.ts`: a short, control-high or fully rounded box filled with, or outlined in, the colour of
 * progress, attention, complete or error), which the strict run fails on; and a control or
 * row shorter than the words it holds, which then draw below its box. Terminals and code
 * editors are left out of the type, radius, spacing and elevation checks, where the standard wants monospace
 * precision, but not out of the overflow and contrast checks.
 *
 * Some routes open a thing before they are measured — the search, the Moebius agent, a form, a menu — because a sheet
 * only shows what it is once it is on the screen. A dialog whose padding is under 16px (`bare`) fails every run, and so does text cut off with no title (`unreachable`) at 1024 and 800px on the pages of `SQUEEZED`.
 * MOEBIUS_DESIGN_ONLY=chat-menu,inbox-search measures those routes
 * alone, which is how one layer is looked at again without the other hundred.
 *
 * `pnpm e2e` does not run this. Run `pnpm build && pnpm audit:design`. Without MOEBIUS_DESIGN_URL it starts the fake
 * server of `../fakeServer.ts`, whose data is the same at every run so pictures and counts compare across a change;
 * with MOEBIUS_DESIGN_URL and MOEBIUS_DESIGN_TOKEN it audits a running server, such as a local `moebius serve`.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, chromium, type Page } from "playwright";
import { afterAll, beforeAll, expect, it } from "vitest";
import { type FakeServer, startFakeServer } from "../fakeServer";
import { findPills } from "./pills";

/** A page to open, and the name its picture is kept under. */
interface Route {
  path: string;
  name: string;
  /** Something to open before measuring, so a sheet is on the screen when it is the thing being judged. */
  open?:
    | "new cycle"
    | "new milestone"
    | "new project"
    | "search"
    | "chat"
    | "new issue"
    | "menu"
    | "terminal"
    | "diff"
    | "changes"
    | "steps";
}

/** One page as the browser measured it. Every list holds findings as plain sentences. */
interface PageAudit {
  /** The name the screenshot is kept under. */
  name: string;
  path: string;
  heading: string;
  status: number;
  overflowPx: number;
  overflow: string[];
  clipped: string[];
  radii: string[];
  spacing: string[];
  /** A surface that is rounded like a card or a control and still draws a line around itself. */
  outlined: string[];
  type: string[];
  weights: string[];
  targets: string[];
  contrast: string[];
  alignment: string[];
  /** A raised surface sitting inside another raised surface. */
  layers: string[];
  /** A control or row shorter than the words it holds, which then draw below its box. */
  spilled: string[];
  /** A sheet with a band of nothing under the last thing in it. */
  chins: string[];
  /** Text cut off by its own box with no title on it or around it, so there is no way to read the rest. */
  unreachable: string[];
  /** A dialog whose content starts closer than 16px to its edge: title, labels and buttons touching it. */
  bare: string[];
  /** A coloured pill that stands for a state: a rounded box filled with the tint of progress, attention, complete or error. */
  pills: string[];
  body: { family: string; size: string; weight: string; lineHeight: string };
}

/** One row of the report: a page under one theme at one width. */
interface Row {
  theme: string;
  width: string;
  page: PageAudit;
}

const out = process.env.MOEBIUS_DESIGN_OUT ?? join(tmpdir(), "moebius-design-audit");
const label = process.env.MOEBIUS_DESIGN_LABEL ?? "run";
const target = process.env.MOEBIUS_DESIGN_URL;
const token = process.env.MOEBIUS_DESIGN_TOKEN;
const strict = process.env.MOEBIUS_DESIGN_STRICT === "1";
/** Route names to measure alone, so one layer can be looked at again without the other hundred. */
const only = (process.env.MOEBIUS_DESIGN_ONLY ?? "")
  .split(",")
  .map((name) => name.trim())
  .filter((name) => name.length > 0);

const themes = ["light", "dark"];
const widths = [
  { name: "wide", width: 1440, height: 900 },
  { name: "narrow", width: 900, height: 700 },
  // The widths a laptop window and a half screen are: where a pane squeezed between the sidebar and the Properties column clips.
  { name: "tablet", width: 1024, height: 768 },
  { name: "small", width: 800, height: 700 },
];

/** Every page of the app, with the ids the fake server answers (repository 1, card 1, run 1000). */
const routes: Route[] = [
  { path: "/inbox", name: "inbox" },
  { path: "/fleet", name: "fleet" },
  { path: "/board/1", name: "board" },
  { path: "/board/1/scripts/Not started", name: "script" },
  { path: "/runs/1000", name: "run" },
  { path: "/chat", name: "chat" },
  { path: "/projects", name: "projects" },
  { path: "/projects/new", name: "projects-new" },
  { path: "/projects/1", name: "project" },
  { path: "/wiki", name: "wiki" },
  { path: "/wiki?page=Team%2FOn+call&view=edit", name: "wiki-edit" },
  { path: "/automations", name: "automations" },
  { path: "/workflows", name: "workflows" },
  { path: "/workflows/release", name: "workflow" },
  // The graph edited on a canvas of its own.
  { path: "/workflows/release/canvas", name: "workflow-canvas" },
  { path: "/workflow-runs/41", name: "workflow-run" },
  // A run that stopped at a node and asks what to do next.
  { path: "/workflow-runs/42", name: "workflow-run-waiting" },
  { path: "/rules", name: "rules" },
  { path: "/retro", name: "retro" },
  // A day whose analysis failed, whose banner offers making it again.
  { path: "/retro?day=2026-10-26", name: "retro-failed" },
  { path: "/experiments", name: "experiments" },
  { path: "/decision-model", name: "decision-model" },
  { path: "/analytics", name: "analytics" },
  { path: "/usage", name: "usage" },
  { path: "/agents", name: "agents" },
  { path: "/skills", name: "skills" },
  { path: "/mcp-servers", name: "mcp" },
  { path: "/models", name: "models" },
  { path: "/plugins", name: "plugins" },
  { path: "/start", name: "start" },
  { path: "/settings", name: "settings" },
  { path: "/terminal", name: "terminal" },
  { path: "/ports", name: "ports" },
  { path: "/people", name: "people" },
  { path: "/tokens", name: "tokens" },
  { path: "/nothing-here", name: "not-found" },
  // The layers that only exist once a person opens them.
  { path: "/inbox", name: "inbox-search", open: "search" },
  { path: "/inbox", name: "inbox-chat", open: "chat" },
  // A card open is a centred window over the dimmed page, and its change opens a sheet of its own.
  { path: "/board/1?card=1", name: "board-card-window" },
  { path: "/inbox?card=3", name: "inbox-card-window" },
  { path: "/board/1", name: "board-new-issue", open: "new issue" },
  { path: "/board/1", name: "board-terminal", open: "terminal" },
  { path: "/board/1?card=5", name: "board-diff", open: "diff" },
  // The same diff read on its own page, where it fills the page instead of a sheet.
  { path: "/runs/1000", name: "run-changes", open: "changes" },
  { path: "/chat", name: "chat-menu", open: "menu" },
  // A conversation with a block of steps, folded and opened, which is the one place the log's 32px row is measured.
  { path: "/chat", name: "chat-steps", open: "steps" },
  { path: "/cycles", name: "cycles" },
  { path: "/milestones", name: "milestones" },
  // The sheets of the pages that kept their form without an inset of their own.
  { path: "/cycles", name: "cycles-new", open: "new cycle" },
  { path: "/milestones", name: "milestones-new", open: "new milestone" },
  { path: "/projects", name: "projects-new-sheet", open: "new project" },
  { path: "/chat", name: "chat-steps-trouble", open: "steps" },
];

/** The pages the issue about narrow widths named: their panes sit between the sidebar and the Properties column. */
const SQUEEZED = new Set(["/decision-model", "/cycles", "/milestones", "/rules", "/wiki"]);

let browser: Browser;
let fake: FakeServer | undefined;
let base: string;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
  if (target) {
    base = target;
  } else {
    fake = await startFakeServer();
    base = fake.url;
    fake.wiki.pushFromElsewhere(
      "Team/On call",
      "# On call\n\nWho answers at night, and how to reach them.\n\n## Rotation\n\nWeekly, from Monday.\n",
    );
  }
  mkdirSync(join(out, label), { recursive: true });
});

afterAll(async () => {
  await browser?.close();
  await fake?.stop().catch(() => {});
});

/** A page of its own, signed in with a one-time code, in one theme at one width. */
async function openPage(width: { width: number; height: number }, theme: string): Promise<Page> {
  const context = await browser.newContext({
    viewport: { width: width.width, height: width.height },
    colorScheme: theme as "dark" | "light",
  });
  const page = await context.newPage();
  const answer = await page.request.post(`${base}/v1/session`, {
    headers: { Origin: base, "X-Moebius": "1" },
    data: { token: fake ? fake.token : (token ?? "") },
  });
  if (answer.status() !== 204) {
    throw new Error(`signing in answered ${answer.status()}: ${await answer.text()}`);
  }
  await page.goto(base);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  return page;
}

/** Opens one page, waits for it to settle, and measures it. */
async function audit(page: Page, route: Route, theme: string): Promise<PageAudit> {
  chatsForSwitcher();
  const path = route.open === "steps" ? stepsChat(route.name === "chat-steps-trouble") : route.path;
  const answer = await page.goto(`${base}${path}`);
  await page.waitForLoadState("networkidle").catch(() => {});
  await page.waitForTimeout(500);
  if (route.open) {
    await open(page, route.open);
    // An overlay animates in; measuring it mid-flight measures an opacity and a scale rather than a layout.
    await page.waitForTimeout(400);
  }
  const found = await page.evaluate(measure, { path, theme });
  const pills = await page.evaluate(findPills);
  return { ...found, pills, name: route.name, path: route.path, status: answer?.status() ?? 0 };
}

/**
 * The path of a chat whose agent worked: it thought, read two files and ran a command, and with `trouble` the command
 * failed, which opens the block of steps by itself. Without a fake server there is no such chat, so the page that is
 * measured is the empty one. The chat is made again for each page, because the sidebar's list is emptied before it.
 */
function stepsChat(trouble: boolean): string {
  if (!fake) return "/chat";
  const id = fake.chat.seed({ title: trouble ? "A run with a failure" : "A run that went well" });
  fake.chat.emit(id, "said", { message: "Fix it" });
  fake.chat.agentStart(id);
  fake.chat.thinking(id, "I should read first.");
  for (const [name, call] of [
    ["a", "r1"],
    ["b", "r2"],
  ] as const) {
    fake.chat.toolCall(id, {
      id: call,
      tool: "read",
      args: { path: `src/${name}.rs` },
      output: `fn ${name}() {}`,
    });
  }
  fake.chat.toolCall(id, {
    id: "b1",
    tool: "bash",
    args: { command: "cargo test --quiet" },
    output: trouble ? "error: test failed" : "test result: ok. 3 passed",
    isError: trouble,
  });
  fake.chat.settle(id, "Done.");
  return `/chat/${id}`;
}

/**
 * The sidebar's list of chats is worth measuring when it has to scroll, and the fake server has no chats until
 * something puts them there. They go back the way they came, so the renders after this one are the renders before it.
 */
function chatsForSwitcher() {
  if (!fake) return;
  // Emptied first, so measuring this render twice puts the same fourteen chats there rather than twenty-eight.
  fake.chat.chats.length = 0;
  for (let index = 0; index < 14; index += 1) {
    fake.chat.seed({
      title: `Chat ${index}: a title long enough to want more than one line in the list`,
      events: [["said", { message: `Chat ${index}` }]],
    });
  }
}

/** Opens the thing a route is measured with, and waits for it to answer. */
async function open(page: Page, what: NonNullable<Route["open"]>) {
  if (what === "new cycle" || what === "new milestone" || what === "new project") {
    await page
      .getByRole("button", { name: what.replace(/^new/, "New"), exact: true })
      .first()
      .click();
    await page.getByRole("dialog").first().waitFor({ timeout: 5_000 });
    return;
  }
  if (what === "search") {
    await page.keyboard.press("Control+k");
    await page.getByRole("dialog").first().waitFor({ timeout: 5_000 });
    return;
  }
  if (what === "chat") {
    await page.keyboard.press("Control+j");
    await page.waitForTimeout(400);
    return;
  }
  if (what === "menu") {
    await page.locator('button[title="Model"]').first().click();
    await page.getByRole("menu").first().waitFor({ timeout: 5_000 });
    return;
  }
  if (what === "steps") {
    // A block of steps that went well is folded; the one with a failure is open already, and is left as it is.
    const folded = page.getByRole("button", { name: /^Worked · \d+ steps?$/ });
    if ((await folded.count()) > 0) await folded.click();
    await page.getByTestId("steps").waitFor({ timeout: 5_000 });
    return;
  }
  if (what === "terminal") {
    // The shell is the reason the board's terminal is a sheet, so the sheet is measured like one.
    await page.getByRole("button", { name: "Display" }).click();
    await page.getByRole("menuitem", { name: "Terminal" }).click();
    await page.getByRole("dialog").first().waitFor({ timeout: 5_000 });
    return;
  }
  if (what === "diff") {
    // The dock keeps one line about a change; the diff is measured where it is read.
    await page.getByTestId("diff-card").evaluate((card) => (card as HTMLElement).click());
    await page.getByRole("dialog").first().waitFor({ timeout: 5_000 });
    return;
  }
  if (what === "changes") {
    await page.getByRole("tab", { name: "Changes" }).click();
    await page.getByTestId("diff").waitFor({ timeout: 5_000 });
    return;
  }
  // A real pointer click: Playwright waits for the button to receive it, so a dock lying over the head bar fails the
  // audit instead of being stepped around.
  await page.getByRole("button", { name: "New issue" }).click();
  await page.getByRole("dialog").first().waitFor({ timeout: 5_000 });
}

/**
 * The measurements, run inside the page. The numbers it allows are the scale of the quiet standard: radii 6 for
 * controls and rows, 8 for cards, 12 for windows and dialogs and full for dots; spacing a multiple of 4, with 6 and 10
 * for the tight places of a dense row and anything 2 and under left alone; and 11, 12, 13, 14, 15, 17 and 20 for type.
 */
function measure(input: {
  path: string;
  theme: string;
}): Omit<PageAudit, "name" | "path" | "status" | "pills"> {
  const radii = new Set([0, 6, 8, 10]);
  const sizes = new Set([11, 12, 13, 14, 15, 17, 20]);
  const canvas: [number, number, number, number] =
    input.theme === "light" ? [255, 255, 255, 1] : [22, 22, 23, 1];
  const skipped = (el: Element) =>
    el.closest(".xterm") !== null ||
    el.closest(".cm-editor") !== null ||
    el.closest('[data-audit="skip"]') !== null;
  const visible = (el: Element) => {
    const style = getComputedStyle(el);
    if (style.visibility === "hidden" || style.display === "none") return false;
    if (Number(style.opacity) < 0.05) return false;
    const box = el.getBoundingClientRect();
    return box.width > 1 && box.height > 1;
  };
  const ownText = (el: Element) => {
    let text = "";
    for (const child of Array.from(el.childNodes)) {
      if (child.nodeType === Node.TEXT_NODE) text += child.textContent ?? "";
    }
    return text.replace(/\s+/g, " ").trim();
  };
  /**
   * True when an ancestor already scrolls sideways. Content wider than the viewport inside such a box is a scroller
   * doing its job — the board's lanes and the usage heatmap are both wider than any screen — and not a spill. The page
   * itself still counts: `overflowPx` says when the document has a sideways scrollbar.
   */
  const scrolled = (el: Element) => {
    for (let node = el.parentElement; node && node !== document.body; node = node.parentElement) {
      if (/(auto|scroll)/.test(getComputedStyle(node).overflowX)) return true;
    }
    return false;
  };
  const name = (el: Element, detail: string) => {
    const node = el as HTMLElement;
    const classes = Array.from(node.classList).slice(0, 3).join(".");
    const label = ownText(el) || node.getAttribute("aria-label") || "";
    return `${node.tagName.toLowerCase()}${classes ? `.${classes}` : ""} ${detail}${label ? ` "${label.slice(0, 40)}"` : ""}`;
  };
  const px = (value: string) => Number.parseFloat(value) || 0;

  type Rgba = [number, number, number, number];
  const parse = (colour: string): Rgba => {
    const parts = /rgba?\(([^)]+)\)/
      .exec(colour)?.[1]
      ?.split(/[ ,/]+/)
      .filter(Boolean)
      .map(Number);
    if (!parts || parts.length === 0) return [0, 0, 0, 0];
    const [r = 0, g = 0, b = 0, a = 1] = parts;
    return [r, g, b, a];
  };
  const over = (front: Rgba, back: Rgba): Rgba =>
    front[3] >= 1
      ? front
      : [
          front[0] * front[3] + back[0] * (1 - front[3]),
          front[1] * front[3] + back[1] * (1 - front[3]),
          front[2] * front[3] + back[2] * (1 - front[3]),
          1,
        ];
  const luminance = (colour: Rgba) => {
    const channel = (value: number) => {
      const s = value / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(colour[0]) + 0.7152 * channel(colour[1]) + 0.0722 * channel(colour[2]);
  };
  const contrast = (front: Rgba, back: Rgba) => {
    const one = luminance(front);
    const two = luminance(back);
    return (Math.max(one, two) + 0.05) / (Math.min(one, two) + 0.05);
  };
  const behind = (el: Element): Rgba => {
    const stack: Rgba[] = [];
    for (let node: Element | null = el; node; node = node.parentElement) {
      const colour = parse(getComputedStyle(node).backgroundColor);
      if (colour[3] > 0) stack.push(colour);
    }
    let seen = canvas;
    for (let at = stack.length - 1; at >= 0; at -= 1) seen = over(stack[at] as Rgba, seen);
    return seen;
  };

  const found = {
    heading: document.querySelector("h1,h2")?.textContent?.replace(/\s+/g, " ").trim() ?? "",
    overflowPx: Math.max(
      0,
      (document.scrollingElement?.scrollWidth ?? 0) - (document.documentElement.clientWidth ?? 0),
    ),
    overflow: [] as string[],
    clipped: [] as string[],
    radii: [] as string[],
    spacing: [] as string[],
    outlined: [] as string[],
    type: [] as string[],
    weights: [] as string[],
    targets: [] as string[],
    contrast: [] as string[],
    alignment: [] as string[],
    layers: [] as string[],
    spilled: [] as string[],
    chins: [] as string[],
    unreachable: [] as string[],
    bare: [] as string[],
    body: {
      family: getComputedStyle(document.body).fontFamily.split(",")[0]?.replace(/"/g, "") ?? "",
      size: getComputedStyle(document.body).fontSize,
      weight: getComputedStyle(document.body).fontWeight,
      lineHeight: getComputedStyle(document.body).lineHeight,
    },
  };

  const edge = document.documentElement.clientWidth;
  const every = Array.from(document.querySelectorAll("body *"));
  for (const el of every) {
    if (!visible(el)) continue;
    const style = getComputedStyle(el);
    const box = el.getBoundingClientRect();
    const text = ownText(el);

    if ((box.right > edge + 1 || box.left < -1) && !scrolled(el)) {
      found.overflow.push(name(el, `${Math.round(box.right - edge)}px past the edge`));
    }
    // Only a box that hides what does not fit is clipped. A box that answers with a scrollbar — `auto`, `scroll` — is
    // doing the thing it was built to do, which is why the script editor and a wide table are not defects. A box that
    // ends in an ellipsis says there is more behind it, which is why a command kept to one line and a name that runs
    // past its bar are not defects either; they still need a title to read the whole.
    const ellipsis = style.textOverflow === "ellipsis";
    if (text && (style.overflowX === "hidden" || style.overflowX === "clip")) {
      if ((el as HTMLElement).scrollWidth > (el as HTMLElement).clientWidth + 1) {
        if (!ellipsis) found.clipped.push(name(el, `cut at ${el.clientWidth}px`));
        // Cut text a person can still read is one with a title, which the browser shows on hover.
        if (el.closest("[title]") === null) {
          found.unreachable.push(name(el, `cut at ${el.clientWidth}px with no title`));
        }
      }
    }

    if (!skipped(el)) {
      for (const corner of style.borderRadius.split(" ")) {
        const value = Math.round(px(corner));
        if (value > 0 && !radii.has(value) && value < 100) {
          found.radii.push(name(el, `radius ${corner}`));
          break;
        }
      }
      // A margin of `auto` pushes a thing to the far end of its row; the space left over is not a spacing someone
      // chose, so it is not measured against the grid.
      const classes = typeof el.className === "string" ? el.className : "";
      const pushed = /(?:^|\s)m[trblxy]?-auto(?:\s|$)/.test(classes);
      for (const property of [
        "paddingTop",
        "paddingRight",
        "paddingBottom",
        "paddingLeft",
        "marginTop",
        "marginRight",
        "marginBottom",
        "marginLeft",
        "rowGap",
        "columnGap",
      ] as const) {
        if (pushed && property.startsWith("margin")) continue;
        const value = px(style[property]);
        if (value > 2 && value % 4 !== 0 && value !== 6 && value !== 10) {
          found.spacing.push(name(el, `${property} ${value}px`));
          break;
        }
      }
      // A window or a dialog that draws a line round itself. A card has a quiet line, but the larger surfaces are set
      // apart by their lightness, a faint edge and in light a soft shadow, which is why only a radius of 10 counts.
      const edges = [
        px(style.borderTopWidth),
        px(style.borderRightWidth),
        px(style.borderBottomWidth),
        px(style.borderLeftWidth),
      ];
      const widest = Math.max(...edges);
      if (widest > 0 && style.borderStyle !== "none" && style.borderStyle !== "hidden") {
        const corner = Math.max(...style.borderRadius.split(" ").map(px));
        if (corner >= 10)
          found.outlined.push(name(el, `${widest}px line at radius ${Math.round(corner)}`));
      }
    }

    if (text && !skipped(el)) {
      const size = Math.round(px(style.fontSize) * 10) / 10;
      if (!sizes.has(size)) found.type.push(name(el, `${size}px`));
      const weight = Number(style.fontWeight);
      if (weight < 400) found.weights.push(name(el, `weight ${style.fontWeight}`));
      const need = size >= 22 || (size >= 17 && weight >= 600) ? 3 : 4.5;
      const ratio = contrast(parse(style.color), behind(el));
      if (ratio < need) {
        found.contrast.push(
          name(el, `${ratio.toFixed(2)} against ${style.fontSize} (needs ${need})`),
        );
      }
    }
  }

  const interactive = 'a[href], button, [role="button"], [role="menuitem"], [role="tab"], select';
  for (const el of document.querySelectorAll(interactive)) {
    if (!visible(el)) continue;
    // 28px is the smallest row of the quiet standard, a row of the sidebar or a small button: the size of a thing you
    // aim at, a control or a link with a fill or its own padding that reads as a button or a row. A link inside a
    // sentence is not that, and holding it to 28px would blow the line apart.
    const style = getComputedStyle(el);
    const role = el.getAttribute("role") ?? "";
    const aimable =
      el.tagName !== "A" ||
      role === "tab" ||
      role === "button" ||
      role === "menuitem" ||
      parse(style.backgroundColor)[3] > 0 ||
      px(style.paddingLeft) >= 8 ||
      px(style.paddingRight) >= 8;
    if (!aimable) continue;
    const box = el.getBoundingClientRect();
    const label = (ownText(el) || el.getAttribute("aria-label") || "").slice(0, 30);
    if (box.height < 28)
      found.targets.push(`${name(el, `${Math.round(box.height)}px tall`)} ${label}`);
  }

  // A row or control is as tall as the words in it. A list that keeps a height to hold shrinks the rows in it
  // down to their own minimum height, and then the last line of a row draws below the box that fills it, rounds
  // it and carries it into the accessibility tree — which is how a menu of a long list reads as broken.
  for (const el of Array.from(document.querySelectorAll(interactive))) {
    if (!visible(el) || skipped(el)) continue;
    const style = getComputedStyle(el);
    // A box that answers with a scrollbar is doing what it was built to do.
    if (style.overflowY === "auto" || style.overflowY === "scroll") continue;
    const box = el.getBoundingClientRect();
    const below = Math.max(
      0,
      ...Array.from(el.children)
        .filter((kid) => {
          const kidStyle = getComputedStyle(kid);
          return visible(kid) && kidStyle.position !== "absolute" && kidStyle.position !== "fixed";
        })
        .map(
          (kid) => kid.getBoundingClientRect().bottom - box.bottom + px(style.borderBottomWidth),
        ),
    );
    if (below > 1) {
      const label = (el.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 30);
      found.spilled.push(
        `${name(el, `${Math.round(below)}px of its own words below its box`)} ${label}`,
      );
    }
  }

  for (const el of every) {
    if (!visible(el) || skipped(el)) continue;
    const style = getComputedStyle(el);
    if (!style.display.includes("flex") || style.alignItems !== "center") continue;
    // A row that wraps has children on different lines; their middles are meant to be apart.
    if (style.flexWrap !== "nowrap") continue;
    const kids = Array.from(el.children).filter((kid) => {
      const kidStyle = getComputedStyle(kid);
      return (
        visible(kid) &&
        kidStyle.position !== "absolute" &&
        kidStyle.position !== "fixed" &&
        kidStyle.alignSelf !== "stretch" &&
        kidStyle.alignSelf !== "start" &&
        kidStyle.alignSelf !== "end"
      );
    });
    if (kids.length < 2) continue;
    const middles = kids.map((kid) => {
      const box = kid.getBoundingClientRect();
      return box.top + box.height / 2;
    });
    const off = Math.max(...middles) - Math.min(...middles);
    if (off > 3) found.alignment.push(name(el, `children ${Math.round(off)}px out of line`));
  }

  // One elevation per stack: a surface with a shadow inside another surface with a shadow reads as two sheets of
  // plastic rather than one layer above the page. The page's own surface, set apart from the ground by its edge, is
  // where the stack starts. A menu, a sheet and a tooltip may be raised — that is what they
  // are — but nothing inside them is.
  const raised = (el: Element) => {
    const shadow = getComputedStyle(el).boxShadow;
    if (shadow === "" || shadow === "none") return false;
    // A field draws its edge as an inset shadow, which sits inside the box and lifts nothing; only a shadow that
    // falls outside the box is an elevation.
    // Tailwind writes every shadow utility as a list of five with the unused ones transparent, which lift nothing
    // either.
    return shadow
      .split(/,(?![^(]*\))/)
      .some((part) => !part.includes("inset") && parse(part)[3] > 0);
  };
  const floats = (el: Element) =>
    el.closest(
      '[role="menu"], [role="dialog"], [role="tooltip"], [data-radix-popper-content-wrapper]',
    ) !== null || getComputedStyle(el).position === "fixed";
  for (const el of every) {
    if (!visible(el) || skipped(el) || !raised(el) || floats(el)) continue;
    for (let node = el.parentElement; node && node !== document.body; node = node.parentElement) {
      // The page's surface is the ground of everything on it: a card on the page is one elevation, not two.
      if ((node as HTMLElement).dataset.surface === "page") break;
      if (!raised(node)) continue;
      const holder = getComputedStyle(node);
      const corner = Math.max(...holder.borderRadius.split(" ").map(px));
      if (parse(holder.backgroundColor)[3] > 0 && corner >= 8) {
        found.layers.push(name(el, `raised inside ${name(node, "")}`));
        break;
      }
    }
  }

  // A sheet ends where the last thing in it ends. When that thing stops short of the sheet's own bottom padding, with
  // nothing centered in the room left over, the band under it is a mistake rather than a margin.
  // A control ends where its own box ends, not where the word inside it does: a row is painted and pressed as a box,
  // and a 40px row with one centred line is the standard's row, not a band of nothing.
  const CONTROL =
    'button, a, input, select, textarea, [role="menuitem"], [role="menuitemradio"], ' +
    '[role="menuitemcheckbox"], [role="option"], [role="tab"], [role="combobox"]';
  /**
   * The bottom of the last thing in a box that a person can see. A box taller than its content — a list with room to
   * spare — ends where its content ends; a box whose content overflows ends at its own edge, because what is past that
   * edge is behind a scroller rather than under a band of nothing.
   */
  const lowOf = (el: Element): number => {
    const kids = Array.from(el.children).filter((kid) => {
      const kidStyle = getComputedStyle(kid);
      return visible(kid) && kidStyle.position !== "absolute" && kidStyle.position !== "fixed";
    });
    if (kids.length === 0) return el.getBoundingClientRect().bottom;
    return Math.max(
      ...kids.map((kid) =>
        kid.matches(CONTROL)
          ? kid.getBoundingClientRect().bottom
          : Math.min(kid.getBoundingClientRect().bottom, lowOf(kid)),
      ),
    );
  };
  /** The top of the first thing in a box that a person can see, the same rule upside down. */
  const topOf = (el: Element): number => {
    const kids = Array.from(el.children).filter((kid) => {
      const kidStyle = getComputedStyle(kid);
      return visible(kid) && kidStyle.position !== "absolute" && kidStyle.position !== "fixed";
    });
    if (kids.length === 0) return el.getBoundingClientRect().top;
    return Math.min(
      ...kids.map((kid) =>
        kid.matches(CONTROL)
          ? kid.getBoundingClientRect().top
          : Math.max(kid.getBoundingClientRect().top, topOf(kid)),
      ),
    );
  };
  for (const sheet of Array.from(document.querySelectorAll('[role="dialog"], [role="menu"]'))) {
    if (!visible(sheet)) continue;
    const style = getComputedStyle(sheet);
    const box = sheet.getBoundingClientRect();
    const pad = px(style.paddingBottom);
    const below = Math.round(box.bottom - pad - px(style.borderBottomWidth) - lowOf(sheet));
    const above = Math.round(
      topOf(sheet) - box.top - px(style.paddingTop) - px(style.borderTopWidth),
    );
    // A band of nothing under the content, with none of it above, is a box that did not stop where its content did.
    if (below > 8 && above <= pad + 8) {
      found.chins.push(name(sheet, `${below}px of nothing under the last thing in it`));
    }
  }

  // A sheet whose children start at its edge. A search palette is flush on purpose and says so.
  for (const sheet of Array.from(document.querySelectorAll('div[role="dialog"]'))) {
    if (
      !visible(sheet) ||
      sheet.closest('[data-audit="flush"]') ||
      sheet.matches('[data-audit="flush"]')
    )
      continue;
    const style = getComputedStyle(sheet);
    const least = Math.min(px(style.paddingLeft), px(style.paddingRight), px(style.paddingTop));
    if (least < 16)
      found.bare.push(name(sheet, `padding ${least}px, under the 16px a sheet is inset by`));
  }

  return found;
}

/** The report as a person reads it: one line per page, then the first findings of every kind. */
function report(rows: Row[]): string {
  const lines = [
    `# Design audit — ${label}`,
    "",
    `Pages: ${rows.length}. Counts are findings; the whole list is in report.json.`,
    "",
    "| Theme | Width | Page | Overflow | Clipped | Radii | Spacing | Outlined | Type | Weight | Rows <28px | Spilled | Contrast | Alignment | Layers | Chins | Pills |",
    "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
  ];
  for (const row of rows) {
    const page = row.page;
    lines.push(
      `| ${row.theme} | ${row.width} | ${page.path} | ${page.overflow.length} | ${page.clipped.length} | ${page.radii.length} | ${page.spacing.length} | ${page.outlined.length} | ${page.type.length} | ${page.weights.length} | ${page.targets.length} | ${page.spilled.length} | ${page.contrast.length} | ${page.alignment.length} | ${page.layers.length} | ${page.chins.length} | ${page.pills.length} |`,
    );
  }
  const kinds: Array<[string, (page: PageAudit) => string[]]> = [
    ["overflow", (page) => page.overflow],
    ["clipped", (page) => page.clipped],
    ["radii", (page) => page.radii],
    ["spacing", (page) => page.spacing],
    ["outlined cards", (page) => page.outlined],
    ["type", (page) => page.type],
    ["weights", (page) => page.weights],
    ["rows under 28px", (page) => page.targets],
    ["contrast", (page) => page.contrast],
    ["alignment", (page) => page.alignment],
    ["stacked elevation", (page) => page.layers],
    ["words below the row", (page) => page.spilled],
    ["bands of nothing", (page) => page.chins],
    ["state pills", (page) => page.pills],
    ["cut text with no title", (page) => page.unreachable],
    ["dialogs without padding", (page) => page.bare],
  ];
  for (const [kind, pick] of kinds) {
    const at = rows.find((row) => pick(row.page).length > 0);
    lines.push("", `## ${kind}`, "");
    if (!at) {
      lines.push("None.");
      continue;
    }
    lines.push(`First in ${at.theme} ${at.width} ${at.page.path}:`);
    for (const each of pick(at.page).slice(0, 8)) lines.push(`- ${each}`);
  }
  return `${lines.join("\n")}\n`;
}

it("audits every page in both themes and both widths", async () => {
  const rows: Row[] = [];
  for (const theme of themes) {
    for (const width of widths) {
      const page = await openPage(width, theme);
      for (const route of routes) {
        if (only.length > 0 && !only.includes(route.name)) continue;
        const each = await audit(page, route, theme);
        rows.push({ theme, width: width.name, page: each });
        await page
          .screenshot({
            path: join(out, label, `${theme}-${width.name}-${route.name}.png`),
            fullPage: true,
          })
          .catch(() => {});
      }
      await page.context().close();
    }
  }

  writeFileSync(join(out, label, "report.json"), `${JSON.stringify(rows, null, 1)}\n`);
  writeFileSync(join(out, label, "report.md"), report(rows));

  const total = (pick: (page: PageAudit) => string[]) =>
    rows.reduce((sum, row) => sum + pick(row.page).length, 0);
  const lines = rows.map(
    (row) =>
      `${row.theme} ${row.width} ${row.page.path}: overflow ${row.page.overflow.length}, clipped ${
        row.page.clipped.length
      }, radii ${row.page.radii.length}, spacing ${row.page.spacing.length}, type ${
        row.page.type.length
      }, contrast ${row.page.contrast.length}, alignment ${row.page.alignment.length}`,
  );
  process.stdout.write(`\n${lines.join("\n")}\n`);
  process.stdout.write(
    `Totals: overflow ${total((p) => p.overflow)}, clipped ${total((p) => p.clipped)}, radii ${total(
      (p) => p.radii,
    )}, spacing ${total((p) => p.spacing)}, type ${total((p) => p.type)}, weights ${total(
      (p) => p.weights,
    )}, rows under 28px ${total((p) => p.targets)}, contrast ${total(
      (p) => p.contrast,
    )}, alignment ${total((p) => p.alignment)}, stacked elevation ${total(
      (p) => p.layers,
    )}, words below their row ${total((p) => p.spilled)}, bands of nothing ${total(
      (p) => p.chins,
    )}, state pills ${total((p) => p.pills)}\n`,
  );
  process.stdout.write(`Pictures and report: ${join(out, label)}\n\n`);

  expect(rows.length).toBeGreaterThan(0);
  // Not behind the strict flag, as `pnpm e2e` is what runs on every change and this is what the issue was about: a
  // dialog is inset at every width, and the pages the narrow widths squeezed keep every word reachable.
  for (const row of rows) {
    expect(row.page.bare, `${row.theme} ${row.width} ${row.page.path}`).toEqual([]);
    if ((row.width === "tablet" || row.width === "small") && SQUEEZED.has(row.page.path)) {
      expect(row.page.unreachable, `${row.theme} ${row.width} ${row.page.path}`).toEqual([]);
      expect(row.page.overflow, `${row.theme} ${row.width} ${row.page.path}`).toEqual([]);
    }
  }
  if (strict) {
    const wide = rows.filter((row) => row.width === "wide");
    for (const row of wide) {
      expect(row.page.overflow, row.page.path).toEqual([]);
      expect(row.page.clipped, row.page.path).toEqual([]);
      expect(row.page.spilled, row.page.path).toEqual([]);
      expect(row.page.pills, row.page.path).toEqual([]);
    }
  }
});
