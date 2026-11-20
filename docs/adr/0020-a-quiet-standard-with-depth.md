# A quiet standard with depth replaces the first one

Status: superseded by [0023](0023-a-quieter-light-first-standard.md). The text below is kept as it was.

The standard of [0016](0016-the-web-app-has-its-own-design-standard.md) drew the web app with 15px body text, radii of 12
and 16, coloured pills for every state, cards inside cards and shadows on every layer. With projects, people and planning
([0018](0018-more-than-one-person.md), [0019](0019-a-project-is-a-board-over-any-repositories.md)) a screen holds far
more, and drawn that way it reads as loud and crowded. The app is to feel premium: quiet, dense and made with care, as
the best trackers are, without copying any of them. This record replaces the look of 0016; its principles (one focal
point, outcomes before machinery, colour carries meaning, the person stays in control) and its rules for behaviour stay.
The drafts it was chosen from are [round 8, 9 and 9b](../design-drafts/README.md).

## Considered Options

- **Keep the standard** and only use it with more restraint: the tokens themselves (15px, 12 and 16, pills) are what
  make a dense screen loud.
- **Flat and minimal**: warm greys, small type, almost no lines, status as small icons
  ([round 8](../design-drafts/round-8.html)). Calm, but plain rather than premium.
- **Minimal with craft**: the same, plus depth, glass, motion, typography, light and detail
  ([round 9](../design-drafts/round-9.html)), each tried on and off.

## Decision

- **Quiet foundation.** Warm greys in both themes, dark as the default. Body text 13px, the text of an issue or a page
  14px, titles 15 to 20px; Inter with its optical sizes, tighter tracking in titles, tabular figures for numbers and
  keys. Radii 6 for controls and rows, 8 for cards, 10 to 12 for windows and dialogs. Rows 28px in the sidebar, 32 to
  36px in lists; the head of a page is one bar of 44px.
- **The right sidebar belongs to the page.** It is the right column of the same surface, set apart by a soft line,
  its head at the height of the page's; folded, and on a page without properties, it stays as a slim column with the
  button that unfolds it and the agent's button. A second floating surface beside the page was tried and read as two
  windows ([prototype](../design-drafts/prototype.html), A).
- **Colour only on small things.** The status of a column, urgent, blocked and errors are small icons in their hue; one
  accent marks links, the chosen thing, focus and a run in progress. Everything else is grey. States that were pills
  ("Live", "Waiting on #24", "Your turn") are a line of grey text or an icon; labels are a dot and a word.
- **Depth through surfaces, not lines.** The page is a surface lying on the sidebar, a shade lighter, flush with it and
  without a line, in light casting a faint soft shadow onto it; in dark it casts none and is set apart by its lightness and a faint
  light edge alone, because a wide shadow on a near-black ground has too few steps of grey and shows as bands. The same
  holds for every large surface in dark: shadows there stay small (a card's, a menu's). It has 6px of air above, below and at the right and all four corners round (radius 12;
  [round 9d and 9e](../design-drafts/README.md), E2 with R2). The
  left sidebar has texture: a fine grain, light from above, the organisation and the chosen row as slightly raised
  surfaces, group names in small capitals ([round 9c](../design-drafts/round-9c.html), T4). The texture lies on the whole
  ground under the page, not on the sidebar alone, so the air and the rounded corners of the page sit on the same
  surface as the sidebar. Lines are few and soft. Cards have
  a quiet line and a short soft shadow that grows on hover ([round 9b](../design-drafts/round-9b.html), K3), without
  gradients or a light edge. The primary button has a slight relief. Overlays (the palette, menus, dialogs, the head
  bar while content scrolls under it) are glass: translucent with a blur.
- **Motion that answers.** About 120 to 240ms, ease-out: a card lifts 1px on hover, the right sidebar slides, the
  palette opens with a small scale, tooltips wait a moment. Nothing moves that the person did not cause, and reduced
  motion turns it off.
- **Light and detail.** A faint wash of the accent at the top of the content, a soft glow on the chosen card, focus as a
  soft ring; tooltips name the shortcut; text wraps evenly; selection is tinted with the accent.
- **A card is named by its project's key**, such as `WEB-24`, unique across projects; an issue of a forge keeps its own
  number beside it.
- The patterns chosen in rounds 1 to 7 (where things are and how they behave) stay; their drawing is replaced by this
  one.

## Consequences

- `web/src/styles/theme.css`, `tokens.test.ts` (radii, type scale, contrast pairs) and the design audit change first,
  in the first issue of P0 of the [plan](../plan/issues-people-wiki-planning.md); every later issue is drawn in the new
  standard.
- `docs/design-web.md` is rewritten for the new look and keeps its rules for behaviour; the screenshots in
  `docs/assets/screenshots/` are taken again.
- Glass and blur cost paint time on weak machines; they are limited to overlays and the head bar, and fall back to an
  opaque surface where `backdrop-filter` is missing.
- A project needs a key, chosen when it is made and kept when it is renamed; links, search and the command line accept
  it.
