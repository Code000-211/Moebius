# Moebius web redesign — decisions signed off by the user

Approved mockups (PNG, 1280x720 @2x) in this folder:
- [`z1.png`](z1.png) Inbox, default state · [`z2.png`](z2.png) Inbox with the agent panel open · [`z3.png`](z3.png) Fleet · [`z4.png`](z4.png) Board
- [`c2.png`](c2.png) card window over the board · [`z5.png`](z5.png) Settings · [`z6.png`](z6.png) Inbox, dark theme

Two decisions made after this list was written win over the lines below: the app keeps the icon package it already uses
(Phosphor, regular weight), so where this file says Lucide it means that; and one restrained red exists, for failures,
destructive actions and removed lines of a diff. The colour values are in [`../../design-web.md`](../../design-web.md),
the standard these decisions became.

Known mockup mistakes, NOT part of the standard: Fleet shows `FLT-` ids (must be the normal issue ids), sidebar counts differ
between images, Settings shows an "Ask Moebius…" field (Settings has none), z6 row text "Plan ready · approve?" (is "Plan ready").

## Guiding rule
A fairly minimal design. Nothing is on screen that helps nobody. No keyboard-key boxes on rows or buttons, no footer hint bars,
no column headers on lists, no captions on values that explain themselves, no count that repeats what is visible, no decoration.
Reference feel: Linear. Dense but not full; nearly monochrome; light theme is primary; used mostly in the Linux desktop app.

## Look
- Text 13 px, rows 36 px. Inter. Mono only for issue ids, code, file names. Tabular numerals.
- Accent indigo (#5e6ad2 family): primary button, active tab (text + 2 px underline, no pill), counts, selected-row edge.
  Orange only for "needs you"; green only for "running". Everything else grey steps.
- Hairlines, not boxes. No card-in-card. One radius family (6/8/10). Lucide icons, one stroke weight. Own status icons
  (needs you, running ring, in review half disc, done, not started dashed ring).
- Main content is one white panel floating on a faintly tinted canvas next to the sidebar, hairline border, no heavy shadow.
- Dark theme: neutral dark (canvas #0f0f10, panel #161617, hairline #26262a, text #ededef, muted #8b8b93, lighter indigo).
- Motion minimal: fades ≤120 ms; reduce-motion switch. Glass/blur only on small menus, tooltips and the palette, and only with GPU.

## Shell
- Left sidebar (Linear style): workspace/server switch on top with search button, Inbox (count), My issues, Fleet ("n running"
  with green dot), Projects, Workspace (Wiki, Cycles, Views, More), Settings at the bottom. No live box.
- Desktop app: no title strip; window buttons (– □ ×) sit in the page header on the right.
- Page header: title, tabs, on the right at most quiet text buttons (Filter, Display) and one primary button.
- Narrow (< ~1000 px): sidebar hides; a button overlays it.

## Lists (Inbox, My issues, project list, Fleet)
- Row: status icon, mono id, full title, one pale reason/activity, then right-aligned: person avatar only when a person owns
  it (agent runs have none), age. Group header rows carry a name only (Needs you / Running / Earlier).
- The one action a row needs is on the row: "Approve plan" on a row that needs you; on Fleet "Stop" and cost only on the
  selected/hovered row. Fleet: the run waiting for you is first; tabs Active / Finished; "Pause all" in the header.
- Loading: skeleton rows. Empty: icon, one very short line ("Nothing needs you"), one action ("Open Fleet").
- Lists are virtualised; each board column scrolls on its own.

## Agent (Moebius)
- Default: one quiet "Ask Moebius…" field docked at the bottom of the main panel.
- When the user asks or Moebius answers, a right panel opens (z2): user bubble, timeline of small steps with icons
  (Thought 6s, Read wiki.rs), answer as free text, dark indigo code block (#1e2038) with header row (language · file · Copy)
  and syntax colours, action buttons, reply field. One close icon. While open, the docked field is gone.
- Chat setting, per person: "Show thinking expanded" (default off; while streaming always open, then collapses). Code open.

## Board and card
- Board: plain column names, flat cards with a hairline: status icon, mono id, full title (wraps), person avatar when owned;
  a second pale line only when an agent is running ("editing engine.rs") or it needs you ("Plan ready", orange).
- A card opens as a centred window over the dimmed (not blurred) board (c2): id, "Open as page", close; title; status icon,
  owner avatar, diff (+84 −12) and "…" on the title row; then the thread as a timeline: comments, moves, "Code run" and
  "Review" as one collapsible line each, and one tinted "Moebius needs you" block with its buttons; reply field at the bottom.
  Inbox and Fleet rows open the same window. New issue, and editing a rule/agent/skill, also use a centred window.

## Settings
- One page: sub-navigation with search and groups You (Profile, Appearance, Notifications, Keyboard), Agents (Agents, Models,
  Skills, Plugins), Automation (Rules, Workflows, Column scripts, Schedules), Insight (Analytics, Retro, Lessons, Export),
  Server (General, Repositories, Runners, Members). The ~20 rarely used areas live here, not in the sidebar.
- Content: label left, control right; a description only where it adds a fact.

## Other pages (decided earlier, same rule applies)
- Wiki: tree, text, outline. Cycles/milestones: timeline. Run page: timeline bar + steps. Analytics: KPIs + table.
  Workflow: canvas with a centred window per step. Palette: list; Tab sends the text to the agent. Terminal: dark, full page.
- Errors and confirmations: toast bottom right (confirmations with Undo). Connect/sign-in: centred card with "Stay signed in"
  (default on).

## Performance (part of this redesign)
Scrolling stutters in the Linux desktop app (WebKitGTK). Known causes: board/inbox/fleet not virtualised; `ScrollFades`
mask-image on scrollers; `.glass` backdrop-filter blur(24px); hover `lift` transitions; per-card infinite animations;
the card drawer squeezing the board (reflow). The redesign removes these.
