# Moebius web redesign — decisions signed off by the user

Approved mockups (PNG, 1280x720 @2x) in this folder:
- [`z1.png`](z1.png) Inbox, default state · [`z2.png`](z2.png) Inbox with the agent panel open · [`z3.png`](z3.png) Fleet · [`z4.png`](z4.png) Board
- [`c2.png`](c2.png) card window over the board · [`z5.png`](z5.png) Settings · [`z6.png`](z6.png) Inbox, dark theme

Two decisions made after this list was written win over the lines below: the app keeps the icon package it already uses
(Phosphor, regular weight), so where this file says Lucide it means that; and one restrained red exists, for failures,
destructive actions and removed lines of a diff. The colour values are in [`../../design-web.md`](../../design-web.md),
the standard these decisions became.

Mockup note: the images above show the first look, a white panel floating on a tinted canvas with 10px corners. That is
superseded by decision V2 (see Look): the sidebar and the page meet at one hairline, the page fills the window, centred
windows are 16px with a soft shadow, and the desktop window has round corners of its own. The mockups are kept as they were
signed off; where they differ from this file and `design-web.md`, the text wins.

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
- Hairlines, not boxes. No card-in-card. One radius family (6/8/10/16). Lucide icons, one stroke weight. Own status icons
  (needs you, running ring, in review half disc, done, not started dashed ring).
- Surfaces (decision V2, replacing the floating panel of the first mockups): a faintly tinted sidebar (light #f9f9fa) and the white page meet at one hairline, and the page fills the window to its edges. The chosen row of the sidebar is a grey pill (radius 8, no border or shadow); the workspace switch has no box. Centred windows (dialog, editor, card window, palette) have radius 16 and a soft shadow (0 0 0 1px hairline, 0 20px 60px rgba(0,0,0,.16)); code and diff blocks in them 10; buttons, inputs and menus 8; rows stay 36px with hairline separators, no pills. The desktop window is transparent with round corners (10), square when maximised. Dark the same way with the dark canvas, panel and hairline.
- Dark theme: neutral dark (canvas #0f0f10, panel #161617, hairline #26262a, text #ededef, muted #8b8b93, lighter indigo).
- Motion minimal: fades ≤120 ms; reduce-motion switch. Glass/blur only on small menus, tooltips and the palette, and only with GPU.

## Shell
- Left sidebar (Linear style): workspace/server switch on top with search button, Inbox (count), My issues, Fleet ("n running"
  with green dot), Chat, Projects, Workspace (Wiki, Views), Build (Agents, Rules), Insight (Insights, Improve), Get started
  (only while a step is missing) and Settings at the bottom. No live box. Changed by [ADR 0024](../../adr/0024-navigation-by-what-you-do.md)
  and issue #700: More, Cycles, Milestones, Intake and Workflows are no sidebar entries (a project owns them); Build and
  Insight are an owner's; the sidebar, the command bar and the Settings navigation read one table (`web/src/app/nav.ts`).
- Desktop app: no title strip; window buttons (– □ ×) sit in the page header on the right.
- Page header: title, tabs, on the right at most quiet text buttons (Filter, Display) and one primary button. The order at
  the right is the same on every page: the page's quiet buttons, its gear or `...` menu, the Moebius button, the primary button.
- Narrow (< ~1000 px): sidebar hides; a button overlays it.

## Lists (Inbox, My issues, project list, Fleet)
- Row: status icon, mono id, full title, one pale reason/activity, then right-aligned: person avatar only when a person owns
  it (agent runs have none), age. Group header rows carry a name only (Needs you / Running / Earlier).
- The one action a row needs is on the row: "Approve plan" on a row that needs you; on Fleet "Stop" and cost only on the
  selected/hovered row. Fleet: the run waiting for you is first; tabs Active / Finished; "Pause all" in the header.
- Loading: skeleton rows. Empty: icon, one very short line ("Nothing needs you"), one action ("Open Fleet").
- Lists are virtualised; each board column scrolls on its own.

## Agent (Moebius)
- Default: a quiet "Moebius" button (sparkles icon + word) in the page header, at the right, left of the page's primary
  button and the window buttons. There is no docked "Ask Moebius…" field on any page. While the panel is open the button
  is pressed (indigo tint, indigo text); clicking it again closes the panel. Ctrl+J toggles, Esc closes; focus goes to the
  reply field on open and back to the button on close.
- The button opens a right panel beside the content (z2; the content gets narrower; under ~1000px it overlays from the right): user bubble, timeline of small steps with icons
  (Thought 6s, Read wiki.rs), answer as free text, dark indigo code block (#1e2038) with header row (language · file · Copy)
  and syntax colours, action buttons, reply field. One close icon.
- Chat setting, per person: "Show thinking expanded" (default off; while streaming always open, then collapses). Code open.

## Board and card
- Board: plain column names, flat cards with a hairline: status icon, mono id, full title (wraps), person avatar when owned;
  a second pale line only when an agent is running ("editing engine.rs") or it needs you ("Plan ready", orange).
- A card opens as a centred window over the dimmed (not blurred) board (c2): id, "Open as page", close; title; status icon,
  owner avatar, diff (+84 −12) and "…" on the title row; then the thread as a timeline: comments, moves, "Code run" and
  "Review" as one collapsible line each, and one tinted "Moebius needs you" block with its buttons; reply field at the bottom.
  Inbox and Fleet rows open the same window. New issue, and editing a rule/agent/skill, also use a centred window.

## Settings
- One page: sub-navigation with search and two groups, You (Profile with Sign out, Appearance, Keyboard shortcuts, My
  tokens) and Server (General, Models, Plugins, Kinds of issues, Members), nine entries; Server is an owner's. Only what
  one sets and leaves alone is here: Notifications (the Inbox is the place), Schedules (the nightly times are rows of
  General; a schedule is a trigger of a workflow, in its project), Repositories and Column scripts (the project's
  settings), Agents, Rules, Workflows, Insights, Improve, Runners and Ports each have a place of their own. Changed by
  [ADR 0024](../../adr/0024-navigation-by-what-you-do.md); the old ~20 areas are gone from here.
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
