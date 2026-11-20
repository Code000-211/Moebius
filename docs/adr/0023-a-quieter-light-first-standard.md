# A quieter, light-first standard replaces the quiet standard with depth

Supersedes [0020](0020-a-quiet-standard-with-depth.md) as the look of the web app. The rules of 0016 and 0020 for
behaviour and words stay in [`design-web.md`](../design-web.md); what changes is how the screen is drawn.

The standard of 0020 made the app denser and calmer than the first one, but it kept a layer of craft on top: grain and a
light on the ground, a wash of the accent, a glow on the chosen card, translucent glass sheets with a 24px blur, cards
that lift on hover, edge fades on every scroller, and a docked inspector beside the board. In use, these were the wrong
trade. The desktop app (WebKitGTK, [0021](0021-a-desktop-app-in-tauri.md)) stutters while scrolling, and the known
causes are exactly these: blur on large surfaces, mask images on scrollers, hover transitions and a panel that squeezes
the board. And the screens still held more than a person needs: key boxes, hint bars, headers over lists, captions that
repeat their value. The decisions and the mockups the user approved are in
[`design/web-redesign/`](../design/web-redesign/DECISIONS.md).

## Considered Options

- **Keep 0020 and fix the performance** by removing the blur and the fades alone. That leaves the dock, the dark default
  and the crowded rows, and still reads as decorated.
- **A different icon library and a bolder look.** Rejected: the app already has Phosphor, and a change of library would
  touch every screen for no gain in clarity.
- **A quieter, light-first standard** in which nothing is on screen that helps nobody (chosen).

## Decision

- **Light is primary, dark is neutral.** The light theme is the one designed first; the dark theme is a neutral dark
  without a tint (canvas #0f0f10, panel #161617), not a warm one, and no longer the default.
- **Nearly monochrome.** Grey steps for everything. One indigo accent for the primary button, the active tab (text and a
  2px underline), counts and the selected row's edge. Orange means "needs you" and green means "running", and nothing
  else is either. One restrained red exists for failures, destructive actions and removed lines of a diff; it is never
  decoration.
- **13px Inter, rows of 36px**, in the sidebar too; mono only for issue ids, code and file names; tabular numerals on
  every figure.
- **Hairlines, not boxes; one radius family of 6, 8 and 10.** The main content is one panel floating on a faintly tinted
  canvas beside the sidebar, with a hairline border and no heavy shadow. Cards on the board are flat. A card opens as a
  centred window over the dimmed board and not in a docked inspector.
- **Motion is fades of 120ms at most, with a switch for reduced motion.** Blur only on small menus, tooltips and the
  palette, and only where there is a GPU. Grain, wash, glow, glass sheets, card lift and scroll fades are dropped.
- **The icon set stays** (Phosphor, regular weight only), with five status icons of Moebius's own.
- **The guiding rule:** no key boxes, no hint bars, no column headers on lists, no caption that explains a value that
  explains itself, no count that repeats what is visible.
- **One table of tokens** in `design-web.md` lists every colour for both themes; the code follows the table.

## Consequences

- `docs/design-web.md` is rewritten and holds the token table. A transition note in it lists what the code still does,
  and the ticket that removes it; the last ticket of the redesign deletes the note.
- `web/src/styles/theme.css`, `tokens.test.ts` and the design audit change in the first code ticket; until then the code
  draws 0020.
- Settings becomes one page with a searchable sub-navigation, so the sidebar shrinks to Inbox, My issues, Fleet,
  Projects and the Workspace pages.
- A list is virtualised and each board column scrolls on its own; this is part of the redesign and not an afterthought.
- The screenshots in `docs/assets/screenshots/` are taken again when the pages have moved.
