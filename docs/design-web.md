# Web app design standard

The standard the web app (`web/`) is built to: what each part of the screen is for, and the exact values it is drawn
with. It is the look of [ADR 0023](adr/0023-a-quieter-light-first-standard.md) and the behaviour of
[ADR 0016](adr/0016-the-web-app-has-its-own-design-standard.md); the app is a web app served by the server and shown
in the desktop app's window ([ADR 0021](adr/0021-a-desktop-app-in-tauri.md)). The decisions it came from, with the
approved mockups, are in [`design/web-redesign/`](design/web-redesign/DECISIONS.md).

This page governs the look, the words and the behaviour of every screen of the web app, so that the code, the tests and
the documentation can cite one thing. Every colour, radius and size is in the token table below, and that table is the
single source: [`web/src/styles/theme.css`](../web/src/styles/theme.css) holds the same names with the same values,
`web/src/styles/tokens.test.ts` holds the tokens to it, and `web/e2e/design/audit.test.ts` (`pnpm audit:design`)
measures every page against it in both themes. `web/README.md` maps the modules the app is built from.

## The guiding rule

Nothing is on screen that helps nobody. A fairly minimal design: dense but not full, nearly monochrome, quiet. The
light theme is the primary one, and the app is used mostly in the desktop window on Linux.

- No keyboard-key boxes on rows or buttons, and no footer hint bars. Shortcuts are named in tooltips and in Settings.
- No column headers on lists. A row says what it is by what it contains.
- No caption on a value that explains itself: "2 running" is not "Running agents: 2".
- No count that repeats what is visible, and no sentence under a title that restates the title.
- No decoration: no grain, no wash, no glow, no gradient, no ornament that carries no information.

A first-time user starts the primary task within 10 seconds. One glance answers what is happening, what matters, and
whether anything needs attention.

## Principles
- One focal point per screen.
- Show outcomes, not machinery.
- Keep the person in control: preserve input, allow cancellation, keep unrelated actions usable during background work.
- Colour carries meaning, and there is little of it. Grey steps for everything, one accent, and three signal colours
  that each mean one thing (see Colour).
- Motion and notifications mark meaningful change only.
- Nothing should make someone feel slow, behind, or overwhelmed.

## Voice
One voice for labels, messages, errors, and generated text.

- Lead with the answer or what changed. Purpose before mechanism; cause and effect when explaining why.
- Sentence case throughout, buttons and headings included. Digits for counts: 3 items.
- Present tense for ongoing work, past tense for completed. Specific verbs: Reading, Comparing, Saving.
- A day is said as "Today", then "Yesterday", then the date itself — `27 Oct`, with the year once it leaves this one. A raw `2026-10-27` in a heading makes the reader do arithmetic to find out how recent it was.
- A gap is said one way per place, from the one helper (`ui/lib/format.ts`): in the age column of a row it is compact — `now`, `2m`, `3h`, `1d` — and in a thread or a sentence it is `just now`, `2m ago`, `3h ago`, `1d ago`. The same place never says it two ways. A time of day keeps the clock that wrote it (`07:33 UTC`) beside the named day.
- A count agrees with its noun from one helper — `plural` and `noun` in `ui/lib/format.ts`. A page does not write `n === 1 ? "card" : "cards"` for a word; `noun` is for the case where the number is already on screen and only the word has to agree.
- A count of what waits for you is said even when it is nothing: the sidebar shows 0, and the empty Inbox says "Nothing needs you" and offers one action. A count of what the machines are doing speaks only when there is something to report, and then in the sidebar as "2 running" in green beside a green dot; the row already names what it counts.
- A step that has finished says how long it took — `Thought 12s`, from `elapsed` in `ui/lib/format.ts` — because a step without a length looks like it is still going.
- A title already says what a page or a box is. There is no sentence under it that says what belongs on the page; a state of the thing — a count, a warning, what is missing — is said where the state is, on the row or in the tab. A category is not a second heading above a title: the navigation already says where a page lives.
- On failure, say what happened and what to do next. End with an action only when there is one.
- Labels: short verbs for actions (Save, Try again), nouns for destinations (Settings, History). Prefer 2-4 words. Icon-only controls get accessible names; helper text states a consequence or constraint.
- One term per concept. Keep a glossary so names match across navigation, controls, messages, and docs.
- The Agents page (tab Agents of Agents | Skills | MCP servers) is a list of 36px rows (name, model and protocol, and at the right Built in or Broken); a row is a link to the agent's own page, `/agents/<name>` (see "The agent's page" below). The old addresses `?agent=` and `?new=1&blank=1` lead to the pages that replaced them (`routes/redirects.ts`).
- **The agent's page** (`app/agents/AgentView.tsx`) is a page, not a window: its header has the way back ("Agents ›", muted, before the name), the agent's name, the tabs **Settings | Instructions | Extensions** (`?tab=`; Extensions with the count of what the agent is given), and at the right a quiet "Delete" (outlined, `--danger-text`, asks first) and the primary "Save", enabled only while something has changed, with the Moebius button before Save as on every page, then the window buttons. A save is a toast and the page stays; leaving with a change asks first ("Leave with unsaved text?" is the wiki's guard, `LeaveGuard`, put as "What you changed on this agent is not saved yet"); changing tab is no way out and does not ask. *Settings* is rows of label at the left and control at the right (Name while a new agent is named, Model, Protocol, Harness, Runner, Thinking, Token budget per card, Engineering skills, Skills, MCP servers, and what a built-in agent brings, read only), with the upstream status line under them, quiet. *Instructions* is one editor that fills the tab, full width and height: mono 13px, long lines wrap, on `--editor-bg` with `--editor-text` (light in the light theme, its own dark tokens in the dark one, never the dark code block); "Write with agent" hands the wish to the Moebius agent. *Extensions* is "Installed" (the Pi packages that name the agent, each with its version, a chip for what it holds, Update when the nightly check found a newer version, and Remove), the extension files of the repository by path, and "Add from pi.dev": a search field, the chips All, Extension, Skill, Theme and Prompt, and result rows with name, version, weekly downloads, a line, kind chip and Add. Adding, updating and removing a package is a proposal, a change of the harness configuration repository that a person merges, so after each request the tab says so above the lists, with the link to the change in the Inbox; a new agent has no package yet, so its Add is quiet until it is saved.
- A new agent starts with one plain choice, before any field: Blank, or From an existing agent (a list of the Coder and the Reviewer, which always exist, and the person's own; never the agents Moebius runs for itself), then Continue. Continue opens the page of a new agent (`/agents/new?blank=1`, or `?from=<agent>` for a copy): the copy has the name empty and the rest filled in, the name is typed in Settings, and a name that is taken (or one of the addresses of Agents, `new`, `skills`, `mcp-servers`) is refused under the head in words; Save creates the agent and opens its page. The page of any agent has a row of the engineering skills, one toggle each, showing the skills of its name until a person chooses; a copy is saved with the list the source has. An agent that has no model says so on its card ("Needs a model", in `--text-muted`, a word and not a coloured tag) rather than showing an empty line.
- The same state has the same words wherever it is said: "Needs you" for what waits for a person (the Inbox tab and group, the sidebar, a run, a Fleet row, a card), "Running" for what an agent is working on right now; "Built in" for anything Moebius brings itself — an agent, a skill, a script, a plugin. A second name for one state ("Your turn", "Waits for you", "Live", "In the server") makes a person learn a synonym.

Never: decorative punctuation and exclamation marks, throat clearing ("Let me", "It's worth noting"), filler praise ("Great question"), narration of internal steps ("I've gone ahead and"), hype ("seamless", "powerful", "smart"), jargon where plain words work.

| Instead of | Write |
| --- | --- |
| Your document has been successfully saved! | Saved. |
| Processing… | Uploading 3 files |
| An unexpected error has occurred. | Couldn't save your changes. Try again. |
| No entities found. | No projects yet. Create your first project. |
| Are you sure you want to remove this item? | Item removed. Undo |

### Generated text
- Sentences of 20 words or fewer. Short replies of 1-3 sentences, under 60 words. Longer explanations go in a structured view.
- Active voice, one topic per sentence, a clear referent for every "this", "that", and "it".
- State only what was observed, checked, or provided, and name real gaps. Recommend an option, with reasons, when a decision is needed.
- Ask only for what the system cannot obtain itself. Replace a confusing explanation with a concrete example.
- Explain through the person, their action, the condition, the result: "Today, changing the filter clears your selection. Saving it separately would keep it."
- Every message stands alone, without earlier messages or outside references.

## Write to fit
- Keep titles, reasons, and action labels visible; wrap rather than truncate. A board card shows the whole title, wrapped. The title of a 36px list row stays on one line and is cut with an ellipsis at its end, and so are compact navigation lists; the whole title is one click away in the card window.
- A line that must stay on one line — a command, a path, a name in a bar — is cut with an ellipsis at the end that runs short and keeps the end that identifies it: the beginning of a name, the end of a path. A line that fits shows no ellipsis and asks for no tooltip either: the tooltip holds the whole of a line that ran out, and repeating a line that is already whole on the screen is noise in a box nobody asked for.
- A scroller shows no fade at its edges. Edge fades are drawn with a mask over the scroller, which costs paint time on every scroll in the desktop window and has no information a scrollbar does not carry; the thin scrollbar says there is more.

- A path is set in the font the list of paths is set in, and monospace is kept for what is literally typed — the line of code a search found, the body of a file. Two lists of the same paths in two typefaces read as two kinds of thing.
- Put the meaningful part of a long path or identifier in view. Size layouts for realistic content, including longer translations.
- Keep live status under 40 characters. Set length expectations wherever content is created.

- A description that is cut short stops at a whole number of lines with an ellipsis (a line clamp), and the whole of it is one click away in the detail. A description that fits is drawn plain.


## Type, colour, shape

### Type
Inter, with its optical sizes (`font-optical-sizing: auto`) and its plainer letter forms (`cv11`); JetBrains Mono only for what is literally typed or named: issue ids, code, file names. Everything else is Inter, including paths shown in a list of paths (two lists of the same paths in two typefaces read as two kinds of thing).

- Body text is **13px**, line height 1.4 in rows and 1.55 in running text. The text of an issue or a page is 14px; a page title in the header is 14px semibold; titles inside content are 15, 17 and 20 (`text-2xs` 11, `text-xs` and `text-sm` 12, `text-base` 13, `text-md` 14, `text-lg`, `text-xl`, `text-2xl`). 11 is for small labels, 12 for ages, counts, reasons and metadata.
- Regular (400) for prose, medium (500) for titles of rows, 550 or semibold for buttons, active tabs and page titles. Titles are tracked a little tighter (-0.005em) and wrap evenly (`text-wrap: balance`); paragraphs wrap without leaving one word on a line.
- **Tabular numerals on every figure**: counts, ages, costs, durations, diff sizes, percentages, ids, dates (`font-variant-numeric: tabular-nums` on the body, so nothing has to remember it). A column of ages or costs lines up, and a ticking figure does not shake its neighbours. Ligatures are off.
- Mono is 12px in a row (an id), 12 to 12.5px in a code block, with ligatures off.

### Colour
Nearly monochrome. The light theme is primary and the dark theme is neutral, without a tint. Both follow the system theme, with an explicit choice in Settings.

- **Grey steps** carry everything that is not a signal: canvas, panel, hairline, text, muted text. Status of a column that is not a signal is a grey mark: not started is a dashed grey ring, done is a grey disc with a check.
- **Indigo accent** is for exactly these: the primary button (`--accent-fill`), a switch that is on (its track, `--accent-fill`), the active tab (its text and a 2px underline, never a pill), counts beside a tab, the edge of the selected row (a 2px bar on its left, with the row tinted `--accent-tint`), the chosen item of the Settings list, the label of the quiet "Approve" button, links, focus, and the in-review mark. Nothing else is indigo. Words in indigo use `--accent-text`; the underline, the edge, the focus ring and the in-review mark use `--accent`.
- **Orange** means "needs you" and nothing else: the needs-you status icon, "Waiting for approval" and other reasons that wait for a person, the "Moebius needs you" block in a card.
- **Green** means "running" and nothing else: the running ring, the dot and "n running" beside Fleet, and `+` figures of a diff.
- **Red** exists for failures, destructive actions and removed lines of a diff (the card window shows `+84` in green and `−12` in red). It is restrained: a small mark, a word, or the label of the Delete button; never a filled banner, never decoration, and never the colour of something that is merely old, blocked or unfinished.
- Pair colour with text, shape or position. A status is always a shape as well as a hue, and an icon-only control has an accessible name. Check contrast in both themes: any text colour is at least 4.5:1 on the surface it is drawn on, so each signal colour has a text form (the `-text` rows of the table) for words, and the brighter mark form is used only for icons, dots and fills, where 3:1 is enough.
- Words that carry a state use the matching text colour only when they are about that state ("Waiting for approval" in orange text); a state that nearly every row is in is a quiet mark and not a word in colour, so the eye lands on the rare row.

### Token table
One table, every colour as a hex value in both themes. The code follows it; if code and table differ, the table is right. Names are the CSS custom properties of `theme.css`; a name not listed here is not a colour of the app.

| Token | Light | Dark | Used for |
| --- | --- | --- | --- |
| `--canvas` | #F9F9FA | #0F0F10 | The sidebar's ground, faintly tinted; there is no air of it round the page |
| `--panel` | #FFFFFF | #161617 | The page, which fills the window, windows, menus, cards on the board |
| `--hairline` | #EBEBEF | #26262A | Dividers, row separators, the panel's header rule |
| `--hairline-strong` | #DCDCE2 | #303035 | A card's border, an input's border, the desktop window's edge |
| `--text` | #1B1B1F | #EDEDEF | Main content, titles |
| `--text-secondary` | #3C3C43 | #C6C6CC | Group names, secondary labels, navigation rows |
| `--text-muted` | #6B6B76 | #8B8B93 | Reasons, ages, inactive tabs, placeholders; text of at least 4.5:1 |
| `--text-faint` | #8B8B96 | #6E6E77 | Icons and the dashes of a timeline; never text, because it is below 4.5:1 |
| `--accent` | #5E6AD2 | #7C85E0 | Active tab underline, selected-row edge, focus, in-review mark |
| `--accent-text` | #5460C8 | #7C85E0 | Indigo words, at least 4.5:1 on every surface of the table: active tab text, counts, links, the chosen Settings item, "Approve" |
| `--accent-fill` | #5E6AD2 | #6068D0 | The primary button and a switch that is on (white label, at least 4.5:1) |
| `--accent-border` | #5560C4 | #7C85E0 | The primary button's edge |
| `--accent-tint` | #ECEEFB | #1C1D32 | The selected row, a selected item in a list |
| `--row-hover` | #F4F4F7 | #1B1B1D | A row under the pointer |
| `--group-fill` | #FAFAFB | #19191B | The row that names a group in a list |
| `--control-fill` | #F1F1F4 | #222226 | A send button, a segmented control's track, the user's bubble |
| `--nav-selected` | #ECECF0 | #1C1C1E | The chosen row of the sidebar, a grey pill with no border or shadow |
| `--needs-you` | #D9690A | #F29442 | The needs-you status icon |
| `--needs-you-text` | #A85A0C | #F29442 | "Waiting for approval" and other orange words |
| `--needs-you-tint` | #FFF8F0 | #2A1F16 | The "Moebius needs you" block |
| `--needs-you-edge` | #F4D8B8 | #4A3523 | The edge of that block |
| `--running` | #1F9D62 | #3EC586 | The running ring, the dot beside Fleet |
| `--running-text` | #177A4C | #3EC586 | "2 running", `+84` |
| `--danger` | #D6455A | #F0707F | A failure's icon, a removed line's mark |
| `--danger-text` | #C23A4F | #F0707F | The label of the Delete button (outlined, a `--hairline-strong` border, never filled), an error sentence, `−12` |
| `--status-todo` | #8A8A95 | #8B8B93 | Not started: dashed ring |
| `--status-done` | #8A8A95 | #72727B | Done: disc, with the check in `--panel` |
| `--status-review` | #5E6AD2 | #8E96E8 | In review: ring with a half disc |
| `--editor-bg` | #FAFAFB | #121214 | The ground of a text editor that holds a person's own text (an agent's instructions): light in the light theme, never the dark code block |
| `--editor-text` | #1B1B1F | #E4E4E8 | Text in that editor, at least 4.5:1 on `--editor-bg` |
| `--code-bg` | #1E2038 | #1E2038 | A fenced code block, in both themes; never a word of code in a sentence |
| `--code-text` | #D8DAF0 | #D8DAF0 | Code |
| `--code-head` | #8C90B4 | #8C90B4 | The header row of a code block (language, file, Copy) |
| `--code-keyword` | #C49BFF | #C49BFF | Syntax: keywords |
| `--code-function` | #7CC4FF | #7CC4FF | Syntax: functions and methods |
| `--code-operator` | #FF9AA8 | #FF9AA8 | Syntax: operators |
| `--code-number` | #FFC27A | #FFC27A | Syntax: numbers |
| `--code-comment` | #858BB4 | #858BB4 | Syntax: comments, in italics |
| `--avatar-1` | #A3563A | #A3563A | A fill of a person's avatar (white initial, at least 4.5:1) |
| `--avatar-2` | #4F7A8C | #4F7A8C | A fill of a person's avatar |
| `--avatar-3` | #7A6A9A | #7A6A9A | A fill of a person's avatar |
| `--avatar-agent` | #2A2C4F | #2A2C4F | Moebius, drawn as a rounded square (`--radius-control`) instead of a circle |
| `--scrim` | rgb(0 0 0 / 32%) | rgb(0 0 0 / 56%) | Behind a centred window; dims the page and does not blur it |
| `--shadow-window` | 0 0 0 1px var(--hairline), 0 20px 60px rgb(0 0 0 / 16%) | 0 0 0 1px var(--hairline-strong), 0 20px 60px rgb(0 0 0 / 55%) | A centred window |
| `--shadow-menu` | 0 8px 32px rgb(0 0 0 / 14%), 0 0 0 1px var(--hairline-strong) | 0 8px 32px rgb(0 0 0 / 50%), 0 0 0 1px var(--hairline-strong) | A menu, a tooltip, a sheet |

The size table, which follows, is the second table of this section.

| Size | Value | Used for |
| --- | --- | --- |
| `--radius-small` | 6px | A chip, a key, an inline code span |
| `--radius-control` | 8px | Buttons, inputs, menus, a sidebar row (the grey pill of the chosen one), a tab's hover |
| `--radius-card` | 8px | Cards on the board, the tinted "needs you" block |
| `--radius-panel` | 10px | A code or diff block inside a window or a page |
| `--radius-frame` | 10px | The desktop window's own corners, drawn by the page (square when maximised or full screen) |
| `--radius-window` | 16px | A centred window: a dialog, an editor, the card window, the palette |
| `--radius-round` | 9999px | Dots, avatars, switches |
| `--row` | 36px | A row of a list, of a sidebar, of a settings list |
| `--row-compact` | 28px | The smallest a control may be; a button in a row is 24px inside a 36px row |
| `--header` | 48px | The page header |
| `--font-size` | 13px | Body text |
| `--sidebar-width` | 224px | The sidebar |
| `--agent-width` | 344px | The agent's right panel |

The dark values of the canvas, panel, hairline, text, muted and the lighter indigo are the ones signed off in `design/web-redesign/DECISIONS.md`; the signal colours and the primary fill are those of the approved dark mockup, adjusted only where the 4.5:1 rule needed it (`--accent-fill`, the `-text` forms). The light values are those of the approved light mockup with the same adjustments: `--text-muted` is a shade darker than the mockup's pale grey, which was 3.4:1 on white.

The colour table is regular so a test can read it: each row has one backticked name, then a light value and a dark value, each a hex colour or a literal CSS value, and hex is compared without regard to case. Pairs that must hold, in both themes: `--text`, `--text-secondary` and `--text-muted` on `--panel`, `--canvas`, `--accent-tint`, `--row-hover`, `--group-fill` and `--control-fill` at 4.5:1; `--accent-text` on the same surfaces at 4.5:1; `--needs-you-text` and `--running-text` on `--panel` and `--danger-text` on `--panel`, `--canvas` and `--row-hover` at 4.5:1; `--needs-you-text` on `--needs-you-tint` at 4.5:1; the white label on `--accent-fill` and on each `--avatar-*` at 4.5:1; every `--code-*` text colour on `--code-bg` at 4.5:1; `--editor-text` on `--editor-bg` at 4.5:1; and the marks (`--needs-you`, `--running`, `--danger`, `--status-*`, `--text-faint`) on `--panel` at 3:1. `--text-muted` is no longer a colour kept below the text range: it is text, at least 4.5:1; `--text-faint` is for marks and never carries words.

The old tones map to the new tokens: attention to `--needs-you*`, progress (a run going) to `--running*`, error to `--danger*`, and complete to the grey `--status-done`. There are no `-fill` or `-border` forms of the signal colours, and no coloured state pills: a state is a status icon and, where a word is needed, its text colour. In the code, `--code-background` becomes `--code-bg` and `--code-string` is dropped, its role taken by `--code-text`. The size names replace the old ones: `--row` for `--row-list`, `--row-compact` (28px) for the 28px of `--row-sidebar` where a control is that small (a sidebar row is now `--row`), `--header` for `--head-bar`, `--font-size` for `--size-body` and `--radius-panel` for `--radius-sheet`; `--row-control` (32px) goes.

### Shape
- **Hairlines, not boxes.** Rows are separated by a 1px `--hairline`; a group is named by a quiet row, not boxed. There is no card inside a card, and no panel inside a panel. A box is for what is picked up or typed into: a board card, an input, a menu, a window.
- **One radius family: 6, 8, 10 and 16, and round.** Nothing else (not 4, not 12). A chip is 6; a control, a menu and a card are 8; a code or diff block is 10; a centred window is 16; the desktop window's own corners are 10; dots, avatars and switches are round. Rows stay dense and square: 36px with a hairline between them, no pills on rows. The only pill is the grey one of the chosen row of the sidebar (8).
- **Spacing** on a 4px grid, with 2, 6 and 10 for the tight places of a dense row. A list row has 16px at its left and right, 10px between its parts.
- **Item rows are 36px, in lists and in the sidebar alike**, and the page header is 48px. Item rows are 36px and group rows 30px. A control is at least 28px, except a tab (26px) and a button that sits inside a row (24px, which does not make the row taller). Rows keep their height whether the sidebar is open or hidden, so nothing moves when the width changes. A conversation's step log is rows of 20 to 24px that sit 4 to 6px apart, because it is a body of small marks and not a list to choose from.
- **Two surfaces meeting at one hairline.** The sidebar is faintly tinted (`--canvas`) and the page is white (`--panel`); they meet at one 1px `--hairline`, and the page fills the window to its edges: no floating panel, no air round it, no border or shadow of its own. In dark the same, with the dark `--canvas` and `--panel`. In the desktop app the window itself is transparent with round corners (`--radius-frame`, clipped, a 1px `--hairline-strong` edge), square while it is maximised or full screen.
- **Depth is one level.** The canvas, the panel, and a window above a dimmed panel. A card on the board is flat: `--panel` with a `--hairline-strong` border. A centred window has `--shadow-window` (a hairline and one soft shadow); a menu has `--shadow-menu`. No two shadows stack.
- Selection is the 2px indigo edge on the left of a row and `--accent-tint`; hover is `--row-hover`. A chosen row of the sidebar is `--nav-selected`. Two fills mean chosen: `--accent-tint` with the indigo edge in a list, `--nav-selected` in the sidebar.
- Focus is a 2px ring in `--accent` with a 2px offset, on every interactive element, in both themes.

### Icons
The app uses the icon package it already has, Phosphor, in the regular weight only, at 15px in rows and controls and 16px for a status icon (14 in dense inline use). One weight, one size per place; no bold, duotone or fill variants of Phosphor. Service logos only identify that service.

The five status icons are Moebius's own, drawn on a 16px grid with a 1.5px stroke, so a status is told by shape as well as by colour and each has an accessible name:

| Status | Shape | Colour |
| --- | --- | --- |
| Needs you | Ring with an exclamation mark | `--needs-you` |
| Running | Ring with a quarter arc, the rest of the ring at 25% | `--running` |
| In review | Ring with its right half filled | `--status-review` |
| Done | Filled disc with a check | `--status-done` |
| Not started | Dashed ring | `--status-todo` |

Pages import icons from one module, `web/src/ui/icons`, by what an icon means; the five status icons are `StatusIcon`. Every column, pull request or tool state maps to one of the five: Backlog and Not started to not started; In progress to running; In review and Ready to merge to in review; Done to done; Blocked, a failure and "waits for approval" to needs you; a name that is none of these to not started. No status icon has an animation class in the code today.

The running ring is the only one that may move, and it moves only while the run is going and not at all with reduced motion (see Motion).

## Shell

The window is a tinted sidebar and the white page, meeting at one hairline (see Shape). In the desktop app there is no title strip: the window buttons (minimise, maximise, close) sit in the page header, on the right.

### Sidebar
224px wide, rows of 36px, 13px text, on its own faintly tinted ground. From the top:

1. The workspace and server switch, with the search button beside it (it opens the command palette).
2. Inbox, with the count of what needs you (a grey number, said even when it is 0), My issues, and Fleet with a green dot and "n running" while anything runs.
3. Chat, after Fleet.
4. Projects: its name leads to the project list and a `+` at its end opens a menu (New project, Add repository); one row per project, opening its board.
5. Workspace: Wiki, Views.
6. Build: Agents, Rules. Agents is one page with the tabs Agents | Skills | MCP servers, each at an address of its own (`/agents`, `/agents/skills`, `/agents/mcp-servers`; the shared piece is `app/shell/SectionTabs.tsx`); Rules is `/rules`. Rules keeps the centred window; an agent has a page of its own (`/agents/<name>`, `/agents/new`). Insight: Insights (tabs Analytics | Usage at `/insights`, `/insights/usage`; `?project=` preselects a project) and Improve (tabs Retro | Lessons | Experiments | Decisions at `/improve`, `/improve/lessons`, `/improve/experiments`, `/improve/decisions`; the training export is the last section of Decisions), each drawn with `SectionTabs`. Both are an owner's; for anyone else the groups are not drawn. Workflows are not here: a workflow belongs to a project ([ADR 0025](adr/0025-workflows-belong-to-a-project.md)), as do Cycles, Milestones and Intake. More is gone.
7. Get started, a row above Settings only while a step is missing, and Settings, alone at the bottom, with its word.

The rows of 1, 2 and 5 to 7 are rows of one table, `web/src/app/nav.ts` (path, label, who is shown it, group, command-bar keywords), which the sidebar, the command bar and the navigation of Settings all read; a page is listed once. Terminal and the quick window are not rows: they are in the command bar and in the project's `...` menu. [ADR 0024](adr/0024-navigation-by-what-you-do.md) decides the list; the mockup is `design/navigation/ia1.png`.

There is no "live" box and no second heading, no repeated counts and no shortcut keys shown. The chosen row is a grey pill (`--nav-selected`, radius 8, no border or shadow), in medium weight; the workspace switch has no box.

Below about 1000px the sidebar is hidden and a button in the page header lays it over the page.

### Page header
48px, one line: the page title (14px semibold), then its tabs, then on the right, in this order on every page: at most quiet text buttons (Filter, Display), the page's gear or `...` menu, the Moebius button and one primary button (New issue), and, in the desktop app, the window buttons after a hairline. `PageHeader` takes them as `actions`, `menu` and `primary`, so the order is the component's and not each page's.

- A tab is a word in `--text-muted`, 26px high; the active tab is `--accent-text` in 550 weight with a 2px underline of `--accent` that sits on the header's bottom rule. No pill, no box. A count after a tab's word is 12px in the tab's colour.
- Text buttons (Filter, Display) have no border; the primary button is `--accent-fill` with a white label, 28px high, with a plus when it makes a new thing.
- A page with nothing to choose between has no tabs.
- **The header's rule is the only line there.** What comes first under the header (a box, a list, a section) starts without a rule of its own: the header already ends in one, and a second hairline a gap below it reads as an empty band. One rule in `styles/tailwind.css` takes the top border off the first thing under `[data-page-header]`.
- **A project's header stays one row of 48px** down to the narrow breakpoint, never wrapping and never clipped. The header is a container: when it is narrower than 60rem (a 1000px window with the sidebar leaves 776px) the quiet buttons give up their words and keep their icon (`header-word` and `header-glyph`: Filter, Display, Moebius), each named by its label for a screen reader. The tabs, the gear, the `...` menu and the primary button stay as they are.

## Lists

Inbox, My issues, the project list and Fleet are one kind of list. Lists are virtualised, so a long one scrolls without cost; a board column is its own scroller. A list has no column headers.

- **Row (36px):** the status icon, the id in mono (12px, `--text-muted`), the full title (medium weight; one line, cut with an ellipsis at the end), one pale reason or activity (`--text-muted`), then, pushed to the right: the person's avatar, only where a person owns the item (an agent's run has none), then the age (12px, tabular, `--text-muted`, right-aligned in a column of fixed width).
- **Group rows** carry a name only ("Needs you", "Running", "Earlier"), 12px medium in `--text-secondary` on `--group-fill`, 30px high. No count, because the rows are the count.
- **The one action a row needs is on the row.** A row that needs you carries a quiet "Approve" button (a 24px outlined button: a 1px `--hairline-strong` border, label in `--accent-text`; the filled form drawn on one Fleet row of the mockup z3 is not the standard); on Fleet, "Stop" and the cost appear on the selected or hovered row only. The run waiting for you is the first row of Fleet. A row opens the card in a centred window.
- **Fleet** has the tabs Runs, Runners and Ports in its header, each at an address of its own (`/fleet`, `/fleet/runners`, `/fleet/ports`); Runners and Ports say how many there are, Runs says no count. Runs has the tabs Active and Finished under the header (`?tab=finished`) and lists the runs of cards, chats and workflows of every project, the project's name on a workflow run's pale line. Runners is the read-only list of the SSH machines with how many runs each has; Ports is the table of the ports the server listens on, with the SSH command that forwards each. Each tab has a skeleton while it is read and an empty state of one short line. On the Runs tab, on the right of the header, "Pause all": a 28px button with a pause icon and a 1px `--hairline-strong` border, the one header button with a border, because it stops work.
- **Loading** is skeleton rows in `--hairline` in the layout the rows will have; nothing is written in them. **Empty** is an icon, one very short line ("Nothing needs you") and one action ("Open Fleet").
- The selected row has the indigo edge and tint; a row under the pointer has `--row-hover`.
- **The code.** One row and one list serve all four pages. `ListRow` and `GroupRow` (`web/src/ui/page/ListRow.tsx`) are the 36px row (class `h-9`, a 1px hairline under it, no shadow; the selected edge is a pseudo element, not a shadow) and the 30px group row; the row takes an optional action, avatar and age and carries the class `group/row`, so a part of it can show on the selected or hovered row only without changing its height. `VirtualList` (`web/src/ui/page/VirtualList.tsx`, on react-virtuoso) draws rows and group rows as one flat `role="list"` of `role="listitem"`s with `aria-posinset` and `aria-setsize` counted over the whole list. The selection is kept by key: up, down, Home and End move it (group rows are skipped), Enter opens it, and Shift with an arrow or a click chooses the range from the ordered keys, not from the DOM. While loading it draws skeleton rows, and with nothing in it the `empty` slot. Tests of a list in jsdom use `mockViewport` (`virtualListTest.ts`, whose file says what it mocks); `web/e2e/list.e2e.ts` builds a page of 2000 rows (`web/e2e/fixtures/`, not part of the app) and checks in Chromium that few are in the DOM, that a row is 36px with no shadow and that the last is reachable.

## The agent

- **Header button.** The agent is opened by a quiet "Moebius" button in the page header, drawn by the shared `PageHeader` so every page has it, Settings included: the secondary button, small, with the sparkles icon and the word, at the right of the header, left of the page's primary button and of the desktop window buttons. While the panel is open it is pressed (`aria-pressed="true"`, `--accent-tint` background, `--accent-text` label); clicking it again closes the panel. `Ctrl+J` toggles the panel too and `Esc` closes it; the focus goes to the reply field on open and back to the button on close. There is no docked field and no shortcut hint beside the button.
- **Right panel.** When the person asks, or Moebius answers, a panel opens at the right of the main panel, 344px wide, set apart by a hairline; the header button shows as pressed while it is open. In a window narrower than 1000px it lies over the page from the right, below the header, instead of beside it. Its head has the mark and name of Moebius and three quiet icons: a new conversation (an action, not a caption above the field), the earlier conversations and the close icon. The earlier conversations are a short list of the person's own conversations with the agent, newest first, that replaces the conversation in the panel until a row is chosen or the arrow of the head leads back; they have no entry in the sidebar. It holds: the person's message as a bubble (`--control-fill`, right-aligned); a timeline of small steps hanging on one thin line, each one line with an icon of its own that says what was done in a few plain words ("Thought 6s", "Read `wiki.rs`", "Ran the tests", never the raw command, which is in the box the step opens to); the answer as free text, not in a bubble; code in a block; the options of a question the agent put with its question tool, drawn as a card under the answer (no button is offered under a plain question: the reply field is right below, and "Approve plan" is for a card that waits for its plan, not for an answer that ends in "?"); and a reply field at the bottom, whose focus is the accent on its edge and no ring. The page beside it keeps its place and narrows. It opens by a fade and not a slide that moves the page.
- **Code block.** `--code-bg` in both themes, `--radius-card`, a header row of 30px with the language, the file and "Copy" (12px mono, `--code-head`), a hairline of white at 7% under it, and the code in 12px mono at a line height of 1.65 with the syntax colours of the table. Code is open by default. The dark block is for fenced code only. **Inline code** (a word of code in a sentence: an issue, a comment, an answer, a Retro proposal) is quiet: mono at 0.9em in `--text`, on `--control-fill`, `--radius-small`, padding 1px 4px, no border, wrapping with the text and never taller than its line. It is one component, `InlineCode`.
- **Thinking.** "Show thinking expanded" is a setting kept per person, off by default: while an answer streams its thinking is always open, and when it ends it collapses to a "Thought 6s" line unless the setting is on.

## Board and card

- **Board.** Columns are named by their plain name, with no count and no header box. Each column scrolls on its own and is virtualised. A card is flat: `--panel` with a hairline border, `--radius-card`, the status icon, the id in mono, the full title (wrapped), and the avatar of the person who owns it, where one does. A second pale line appears only when an agent is running on it ("editing engine.rs") or it needs you ("Waiting for approval", in `--needs-you-text`).
- **Card window.** A card opens as a centred window over the dimmed board: `--scrim` (the board is dimmed, not blurred), `--radius-window`, `--shadow-window`, about 800px wide. Its header row has the id (mono), "Open as page" and a close icon. Under it, on the title row: the title, then the status icon, the owner's avatar, the diff (`+84` in `--running-text`, `−12` in `--danger-text`) and a "…" menu. Then the thread as a timeline: comments, moves ("Moebius moved this to In progress") and "Code run" and "Review" as one collapsible line each, and one tinted "Moebius needs you" block (`--needs-you-tint`, a 1px `--needs-you-edge`, `--radius-card`) with its buttons. A reply field is at the bottom. A row of the Inbox or of Fleet opens the same window, and so do New issue and the editing of a rule, an agent or a skill.

## Settings

One page. At its left a sub-navigation of two groups, an icon before each entry and no search field, in this order: You (Profile, with Sign out; Appearance; Keyboard shortcuts; My tokens) and Server (General, Models, Plugins, Kinds of issues, Members), nine entries in all (`design/navigation/ia4.png`). Server is an owner's: anyone else sees only You. The groups are read from the same table as the sidebar (`nav.ts`). Nothing in Settings is a tool or a report: Notifications are the Inbox (whose empty state says what arrives there), the nightly times (the Retro and the trial window) are rows under "Nightly" in General, a schedule is a trigger of a workflow in its project, Repositories and Column scripts are tabs of the project's settings, Runners and Ports are tabs of Fleet, and Agents, Rules, Insights and Improve have rows of the sidebar. What plugins ask of the whole server is a section per plugin on the Plugins page. Every removed address redirects (`routes/redirects.ts`). A group name is a small muted label, and the chosen item is `--accent-text` on `--accent-tint`, in medium weight.

Appearance has these rows: Theme (System, Light, Dark), Show thinking expanded, Wrap long lines (off by default: a line longer than its code block scrolls sideways, and on it breaks at the block's edge in chat answers, card threads and run pages; kept per person with the other preferences), Reduce motion and Blur on menus.

The content is rows of a label at the left and its control at the right, separated by hairlines. A description appears only where it adds a fact ("While an answer streams it is always open"). Settings has the same header button as every page. A project's settings (General, Members, Repositories, Columns; `design/navigation/ia3.png`) have the same sub-navigation: the project's name over four rows, each with an icon, drawn by the same row (`app/settings/SubNav.tsx`).

## Other pages

The same rules apply. Wiki: the tree at the left, the text, an outline of its headings at the right. Cycles and milestones (tabs of a project, under its header): a timeline. A run: a timeline bar and its steps. Analytics: figures and a table. A workflow: the canvas, with a step opened in a centred window; its definition and its runs are in the project's Workflows tab (below). The palette: a plain list; Tab sends what was typed to the agent. The terminal: dark in both themes, a full page. Errors and confirmations are a toast at the bottom right (a confirmation offers Undo). Sign-in and connect: a centred card with "Stay signed in", on by default.


### The Workflows tab of a project

A workflow belongs to one project ([ADR 0025](adr/0025-workflows-belong-to-a-project.md)), so everything about workflows
is in that project's tab (`web/src/app/workflows/WorkflowsTab.tsx`; mockup `docs/design/navigation/ia2.png`, which the owner
extended: the definitions moved from Build into the tab). Under the project header, two lists drawn as ordinary rows on
the panel, with no card:

- **Workflows**: one 36px row per definition of this project, with the workflow icon, its name (a link to the
  definition at `/projects/:id/workflows/:name`, which links to the canvas at `.../canvas`), what it is for in muted
  text, what starts it (Manual, Schedule, Event, Webhook) and a status tag with the age of its last run. A workflow on a
  webhook trigger shows the address it answers on, `/api/hooks/projects/<project>/<path>`, in the code font beside its
  purpose with a Copy button (`hook_address`; the address without the project is deprecated and never shown). The
  Webhooks panel of Settings holds only the forges' hooks, so a workflow's hook lives here, in its project: a private
  project's hooks are only read by those who can open it. "New workflow"
  (a blank canvas) and "New scheduled workflow" (the automation template, in the editor window) sit at the right of the
  section title, as secondary buttons.
- **Runs**: a tab row `All | Scheduled | Failed`, in the address as `?runs=`, over this project's runs newest first,
  each a row of the workflow's name, `repository · status` in muted text (`failed at "deploy"` in the error colour) and
  its age at the far right. A row opens the run, whose page links back to this tab. **Scheduled** first lists the
  project's schedule-trigger workflows with their schedule and when each fires next (`in 3 h`), a word where one is
  off or failed, and Edit, which opens the automation in the editor window (`?automation=<id>`); the scheduled runs
  follow. A schedule-trigger workflow that is not an automation (a trigger with no `chat` node, or an older file) shows
  `Schedule · next in 5 h · last fired 19h ago` in the same place: the server works the next firing out from the trigger's
  cron in its local time, the way the scheduler fires (`next_firing` and `last_fired` on `GET /v1/projects/{id}/workflows`),
  and the app does not guess it; with none it says `Schedule · no next firing`. Saving a workflow on the canvas, and every `workflow_*` event, read the
  automations again, so the list joined to the definitions stays current.

The one primary button of the header, "Run a workflow here", is at its far right. It opens a dialog that asks which of
the project's manual workflows to run, the repository (only the project's own, the home one first and chosen) and one
field for each input the workflow's manual trigger names. A trigger input of type `repository` is a choice among the
same repositories. The server's refusal is shown in the dialog in its own words. Delete on a definition shows the
server's refusal in the same way.

The old Workflows and Automations pages are gone: `/workflows`, `/workflows/:name`, `/workflows/:name/canvas` and
`/automations` redirect to the project named by `?project=`, else the one opened last or the only one, else the list of
projects.

## Components
- Buttons: one clear primary action, quieter alternatives, consistent sizing. A button is as wide as its label, never the width of the row it happens to sit in. An action that starts a new thing says it with a plus, on the page header and in an empty place alike, so the same verb is drawn the same way wherever a person meets it. A destructive action is the `danger` variant wherever it appears, including the button that opens the question.
- A delete is asked once, in a dialog (`ui/page/ConfirmDelete.tsx`) that says what is removed and, for a thing that is going, that it is stopped first. Its buttons are Keep and the danger Delete; the server's refusal stays in the dialog. The Delete that opens it sits in the same place on every page: in the actions of the thing's own header or row, as the danger variant. After the delete the list updates from the event, and the page of what was deleted is left for its list.
- One thing that is on or off is one switch (`ui/page/Switch.tsx`), never two buttons offering "On" and "Off": the pair reads as two actions rather than one state, says nothing at all while neither is pressed, and is twice the surface to hit. A switch carries no "On" or "Off" beside it, as in the mockups; it says its state by the end the knob is on and, for a screen reader, by its role and `aria-checked` (the accessible name is what is switched) — never by the colour of its track alone. An enabled switch that is off has `--control-fill` for its track and `--text-muted` for its edge and knob (3:1 against the panel in both themes), which tells it from a disabled one (a muted track) and from one that is on (indigo). The arrow of a send button is `--text` on `--control-fill`, `--text-secondary` while it cannot send.
- Inputs: clear label, visible focus, preserved content, errors beside the field. Let them grow when content needs it. A field is the colour of its panel (`--panel`) with a 1px `--hairline-strong` border, so the thing to type into is seen by its edge.
- Tooltips supplement and never carry essential information. Toasts appear only when the current screen does not already show the result. Errors and confirmations are toasts (`ui/page/Toast.tsx`, `useToast()`), not inline banners: at the bottom right of the panel on a solid surface, fading in, going by themselves after 5 seconds (an error after 8) or by their close icon; an error is an alert and a confirmation a status, and a confirmation of what can be taken back offers Undo (blocking a card, which unblocks it). The connection banner stays a banner, because it is a state and not a result. Dialogs interrupt only for decisions that genuinely need it, and never trap. Windows keep the dimmed screen behind them recognizable.
- A dialog or centred window is inset by `DialogContent` itself (`ui/components/Dialog.tsx`: 24px of padding and a gap of 16px), so a dialog puts its title, fields and buttons straight inside it and sets neither; one that wants its content flush (the search palette) says `p-0` and `data-audit="flush"`. A window sits over a dimmed page and is closed by its close icon, by Escape and by a press on the dimmed page. Below about 1000px the sidebar hides behind a button, and a pane lays out by its own width with a container query rather than the window's. The design audit measures 1440, 1024 and 800px and fails a dialog without padding and, at 1024 and 800, cut text with no title on the Decisions, Rules and Wiki pages and the Cycles and Milestones tabs of a project.
- A panel or window ends where the last thing in it ends: nothing sits in a band of empty space under the content, and a list that scrolls reaches the panel's own padding. So does a box beside a column that grows: a file tree held to the height of the column next to it leaves a band under its last row, which reads as a box that failed to fill, so it stops where its last row stops (`app/projects/FilesPanel.tsx`).
- Work that wants the height of the page — a shell in the clone of a repository — is a page of its own (the terminal, dark in both themes) rather than a band under the content of another page, so the work gets the height it needs instead of a strip that has to be scrolled inside a strip.
- A search that filters a column sits above the column it filters, not over it: a window opened to look for a file hides the very thing the letters are narrowing, and every search closes what it was about. The field waits for the typing to settle, so a search costs one per word and not one per letter, and a search still running keeps the hits it had, so the list does not jump to nothing between two keystrokes (`app/projects/FindPanels.tsx`). A list of what it found floats under the field rather than pushing the rows below down, because the rows below are what the search is reading against.
- A page that is a thing inside another thing — a repository inside the list of projects — carries the way back on the left of its name, which is where a hand goes looking for it. It is an icon named by a tooltip, because the room belongs to the name, and the title beside it is cut rather than wrapped so the head stays one line (`ui/page/PageHeader.tsx`). A page that is the top of its own group is not such a page; Settings is one page whose sub-navigation is always shown, so it has no way back.
- An empty place names the next action and offers it ready-made — two or three starting points that fill the field or start the thing — rather than explaining what an empty place means.
- A navigation entry leads where a person wants to be, not to an empty field. The entry stays standing across everything it leads to, including an address of its own. The chosen row of the sidebar is `--nav-selected`, and the hover of the navigation is the shared `--row-hover`.
- A path shows the part that names the file (`SKILL.md`), with the whole of it kept where it can be read — the tooltip, the title, the detail beside it.
- A file that is read as often as it is edited — a skill, a page of the reference — is offered drawn as well as raw (`ui/page/MarkdownBody.tsx`), and opens as the text: the text is what a person went there to look at, whether or not they can change it, and the drawing is one tab away. How it is read is held above the file, so moving along a list of them never asks for the choice again.
- A field is never blank. What stands now is written in words, including the answer that comes from the agent or the server rather than from a pick: "Server default", not an empty box that leaves the person guessing whether the page loaded. Where a field is empty because what it writes to is gone, it says so and names the way on — "Pi ended this chat. Start a new chat to continue."
- An action that leaves nothing to look at is an item of the list it belongs to rather than a page of its own, not a screen. Where Sign out is offered is not decided by the redesign; the issue that builds Settings places it (default: the You group).
- The way to Settings is a row at the foot of the sidebar, alone, with its word, on every view of it. The foot is the one row the sidebar never exchanges, so a hand that learned its place keeps it.
- The control that switches the workspace or the server sits in the sidebar's head, beside the search button, where a hand goes looking for it. Where the sidebar is hidden on a narrow window, the button that shows it is in the page header at the left.
- A field that can be left blank says what blank means — "No limit", "This one or the secret", "Set both, or neither" — in the place a blank would otherwise sit. A key with a default is said the same way ("Default 3"), so an empty field is never a guess.
- The project list is one kind of list like the others (see Lists; `app/projects/ProjectsPage.tsx`): a row with the project's mark (the first letter of its key, in grey), its name and its key in mono. The redesign did not sign off a grid of tiles, and the issue that builds the page settles what else a row says (repositories, members, open cards). A row is one link, to the project's settings (`app/projects/ProjectSettingsPage.tsx`, reached from the gear of the project's header, with a sub-navigation at the left as Settings has it (ia3.png) holding four areas: General with name, key, Private, Intake and the kinds offered, Members, Repositories whose rows open the repository's page, and Columns with the one editor of column scripts, opened at a column from the board's column menu; only an owner edits, anyone else reads; navigation is ADR 0024); it carries no buttons, since the board is in the sidebar's switcher and a repository is deleted on its own page, one click from the Repositories tab. A new project's key is offered from its name as it is typed, and kept once the person types their own.
- A wiki page has three columns (`app/wiki/WikiPage.tsx`): the tree at the left, with the organisation's pages and then a group per project under its name, folders before pages, "New page" at the foot of each group; the page in the middle, drawn as Markdown under its name and who edited it when, with History, Move, Delete and Edit in its head; and at the right "On this page", the page's headings, which scroll to their place. The editor stacks the Markdown and its preview, because the outline keeps its column while editing, with a line for the commit's message; Save and Cancel are pinned to the foot of the text column, always in view. An older version is read in place under a notice that says whose it is and leads back to the page as it is.
- A list with nothing in it takes no column of its own: the empty state that answers it — the one holding the button that makes the first row — uses the whole width (`ui/page/List.tsx`).
- A collapsible row opens its own panel at the row's width: the box of what a tool printed starts and ends where the row it belongs to does. A list of separate items under a heading may still step in, because those are children rather than one box opening.
- A mark and the line of text beside it share that line's middle. A pill nudged down to meet the first line of a row reads as a mistake as soon as there is a second row.
- Skip unnecessary nesting, repeated containers, crowded toolbars, and long undifferentiated settings pages.

## Shared components
One implementation of each control, in one place: ``web/src/ui/``.

- Use a shared component before building a new one. When none fits, extend it with a variant rather than copying it into a feature.
- Components read color, spacing, radius, and type from named tokens (Surface, Primary text, In progress), never raw hex or one-off values.
- Consistent names across the set. Variants: primary, secondary, ghost, danger. Sizes: sm, md, lg. States: default, hover, pressed, focus-visible, disabled, loading.
- Every interactive component ships with an accessible name, visible focus, keyboard operation, reduced-motion behavior, and both themes. Features do not reimplement these locally.
- A scrollbar stays quiet: thin, no arrows, no track of its own colour, and a thumb only a little stronger than a hairline. It is painted only while the pointer is inside the pane it scrolls, since a bar that is always there is furniture and sits over the words of a narrow pane; only its paint is gated, so the room it takes never changes and nothing in the pane slides when a hand arrives or leaves (`src/styles/tailwind.css`). A screen with nothing to hover — a phone, a remote desktop with no mouse — keeps the thumb, because there the bar is the only thing that says what is off screen.
- Dropdowns, selects, and menus: the trigger shows the current selection, Escape closes, arrows and typing work, short lists stay inline and long ones become searchable, and the list aligns to the trigger without leaving the viewport. The chosen item is medium weight on `--accent-tint`, not marked with a circle or a tick that leaves the row looking unanswered. The caret sits inside the trigger's own box, and one control (`ui/page/Field.tsx`) serves every choice in the app — a page does not reach for the browser's `<select>`. A menu scrolls once, in its own surface: a list inside it that caps its own height makes a second scroller beside the first, and a column of a fixed height shrinks the rows it holds down to their minimum height, so their words fall outside the row's own fill. A scroller is also the box that what it scrolls belongs to: a column with its own scroll is `relative`, or a thing placed absolutely inside it — a field's hidden hint, say — keeps the page as its containing block, stands where its place in the scrolled content falls outside the window, and gives the whole screen a second scrollbar beside the column's own. A column that scrolls reaches the edge of the window it is read in, rather than stopping short above a band of the page's own padding (`ui/page/List.tsx`). A choice whose value already says what it is — a model's name, a reasoning level — shows the value alone, with its label kept as the accessible name and as the head of the list it opens; a label is for a value that could be anything.
- A property is edited where it stands (`web/src/ui/page/EditableProperty.tsx`): in the card window and on the card page each is a row of its name and its value, or a quiet control on the title row, and the value is a quiet button named by both ("State Open") that opens the value's own editor — a choice or a person from the app's menu, a date, a number or a line of text in a field put in the button's place. It saves that one value alone: on a choice, on Enter or when the field is left; Escape leaves the field unsaved, goes no further (the window behind it stays open), and the focus returns to the value. A property of several values (labels, blockers) shows each with its own remove button and a "+" that opens the field to add one. While a save is out the value says it is busy and disabled without giving up the focus. What the server refused is said under the property that asked, in `--danger-text`, and is the description of its controls, so a person hearing the page is told it where they are; the value stays what the server has. No form of several properties is saved together, and no Save button stands under them.
- A control is given its state by the caller, not worked out inside a tooltip: a tooltip's trigger folds a function `className` into text, which leaves the row it wraps with its link and no shape.
- Keep components presentational. Data and feature logic live outside; components take values in and report intent out.
- Feature-local components stay in the feature directory and move into ``web/src/ui/`` only after a second real use. Describe each shared component in a doc comment at the top of its file, with one example of correct usage, and list its folder in the layout of `web/README.md`.
- Changing a shared component updates every call site in the same change, or adds a variant and deprecates the old one instead of breaking consumers.

## Interaction
- Respond immediately. Use optimistic updates when failure can be handled safely.
- Prefer Undo, and only when the action is actually reversible. Confirm only significant, irreversible actions.
- Reveal gradually: a glance, a closer look, a detailed view.
- A control that carries what is on screen somewhere else — Copy — waits for the block it belongs to be hovered or focused (`copy-quiet` and `reveal-copy` in `web/src/styles/tailwind.css`). The words are the content; a quiet button beside every message, tool call and code block at once is noise. A screen with nothing to hover keeps the control in place.
- A state is said once, by the thing that owns it: a chat that ended carries the word in its own tag, so the line under the composer adds nothing — and the button that goes on from it is labelled for what it now does (Continue), because the state and the action are two different things a person asks where they are; a count is not repeated by a sentence under the heading that counts it, the day a report was read on is said by the list that chose it rather than again in the title of every card on it, and the number at the end of a row that already names what it counts is a number — "Chat 2", not "Chat 2 new".
- An agent's message keeps the newlines it was written with, because a selection copies what is drawn: a break drawn as a space is gone from anything pasted out of the screen, and a transcript pasted into a field arrives as one line. A Markdown file is the other kind of writing — hard-wrapped by whoever wrote it — so there a newline stays soft and only a blank line makes a paragraph.
- The field that writes to an agent names the agent — "Reply to Moebius…" in the right panel — which is why no pill beside it repeats the name. The conversation started in the panel is a chat like any other — the panel is a window onto it, not a second kind of chat — and it is unread until it is read somewhere (where past conversations are listed is not decided by the redesign; default: in the agent's panel): reading it in the panel marks it read, closing the panel does not, and an answer that arrived while it was closed is still waiting.
- A sheet or panel at the right edge starts below the page header (48px, `top-12`) and never covers the window buttons, which sit in that header at the top right (`ui/page/Sheet.tsx`, tested in `Sheet.test.tsx` and, in a desktop window, in `e2e/packages.e2e.ts`).
- The package panel (`web/src/packages/`) is the Plugins page's: it opens in the agent's right panel, in the agent's place: opening one closes the other and keeps its state, Escape or the close icon closes it, and its state — the search, the tab, the package being read — waits where it was left until the page opens it again. It is pointed at one catalog at a time: the Plugins page's "Browse packages" opens it on the Moebius registries (plugins, workflows, rules, skills). The pi.dev catalog is not in it: an agent's Extensions tab has it, full width, in the page. The search field has the focus when the panel opens. It has two tabs, Browse (a search field that waits for the typing to settle, a kind filter, results with name, kind, version, description, "Installed 1.0.0" and "Update to 1.1.0", downloads for Pi packages) and Installed (Update, Remove, and Update all for Pi). A package opens in place of the list, with a way back that names where it came from. What an action did is said in the panel by a notice in the API's own words and never as a broken page: `Installed <name> <version>.`, the plugin's own error when it does not run, "Restart the server for it to take effect.", a registry that could not be read beside the results of the others, a refusal with its sentence, and for a session that is not a person's, whose it is not. A plugin installs at once, so removing one asks first; a workflow, rules or skills package and every Pi package is a change a person merges, so the panel says so before the press, never asks twice, and after it links to that change in the Inbox (`/inbox?change=<id>`, the Inbox narrowed to it), closing itself for the page it leads to. The `package_*` events and `extension_update_available` (the nightly check of the Pi packages, whose item is in the Inbox) read it, and the Plugins page's list of installed packages, again. A workflow, rules or skills package is removed only as a proposal, so it is said as `Proposed removing <name>.` with the link to the change and is not asked about first; a plugin goes at once, so it is asked about once.
- A control that cannot be used and has a reason carries it where a mouse, a keyboard and a screen reader all reach it, never as a line of text beside it and never in the browser's own tooltip box. A `title` draws a strip of somebody else's design system over the page it sits on, in colours and a font and a delay nobody chose, and a disabled `<button>` is out of the tab order, so neither it nor its `title` reaches a keyboard. `ui/components/Button.tsx` therefore keeps such a button focusable (`aria-disabled`, every click ignored), shows the reason in the app's own tooltip on hover and on focus, and keeps it for a screen reader in `aria-describedby`. A control that simply has nothing to do is disabled and says nothing — Save with nothing waiting is that control. Where it would change nothing whatever it did, it is not offered at all: a chat window already showing a conversation nobody has started offers no way out of one. Keep errors beside the affected content, with a useful next step.
- Empty states say what belongs there and how to begin, unless the next step is already obvious: an icon, one very short line and one action. One is a card only where it stands on the page itself; on a surface that is already one — a lane of the board, a panel — it is `plain` (`ui/page/Empty.tsx`), because a second surface under the same sentence reads as a card that has lost its words. Its heading names the place, so a place already named on screen — a column under its own header — is the sentence alone, and a state that is the whole of an empty place is centred in it rather than left at its top.
- Loading placeholders match the expected layout; keep existing content during refreshes. Only the first read gets one (`ui/page/Skeleton.tsx`): rows where the list of rows will be, lanes where the board's lanes will be, lines where a file's lines will be, so the page fills in place instead of moving under the eyes. The bars carry no words of their own — a sentence about to be replaced by a different sentence is a promise nothing can keep — and they are filled with the hairline token, the tint a divider is drawn with, so a bar reads as a line not drawn yet rather than as a control that cannot be used. What the old spinner said out loud is still said: the placeholder is a status region whose sentence is spoken rather than shown, which is also why, where there is no layout to mirror and real work is happening — the app reaching a server for the first time, an agent's turn still being written — a still mark with its status text stands in, since the running ring is the only thing that may move.
- A card has one ⋯ menu for what a person does to it besides deciding (`app/board/CardMenu.tsx`): Block… or Unblock, and Delete…. It stands on the title row of the card window, is a button with an accessible name ("More actions for #12"), opens with Enter or Space and closes with Escape, and keeps every key and click to itself, because the card it sits in opens on Enter, is picked up on Space and the window closes on Escape. A choice that needs words opens a dialog of the page (`CardDialogs.tsx`): Block… asks for a reason and says it is optional; Delete… asks once, says what goes and, for a card of a forge, that the issue stays open there and is not made a card again, and its button is the danger one with its own label ("Delete card"). A refusal stays in the dialog beside the buttons.
- Blocked and waiting are two states with two words. A card a person or the Moebius agent blocked carries the word "Blocked" in `--text-secondary`, with a grey mark (no hue, and never red or orange: it is not a failure and does not need you), and the reason in its tooltip and in its properties ("Blocked"); a card that depends on other issues says "Waiting on #12", in `--text-muted`, also without a hue. Its properties keep the two apart: "Why it waits" is the reason the card's script gave, and "Waiting on" lists the issues it depends on. A blocked card is never the next one to start, and a drop on a column says to unblock it first. A card that was deleted takes its window with it: the window closes rather than opening another card.
- Preserve navigation, drafts, selections, and reading position.
- A long conversation opens at its last message and is read backwards a page at a time (`chat/useThread.ts`): draining a run's whole log before its first line appeared left a page showing nothing for a second or two, and nobody scrolls up to a message until they decide to. What is above the window is fetched when a person reaches the top of the list — the gesture that asks for it — and it is added without moving the turn being read, because a page of history that shoves the sentence in front of them off the screen is not a page they asked for. Nothing is offered for that scroll: a row above the oldest turn that waits to be pressed repeats a gesture the person has already made, and one that promises the rest of a chat which has none is worse. It appears only while the page above is on its way, drawing the turns coming, or after it failed, with the reason and the way to ask again. Reaching the top is the only way to read older messages, so a first read that stops at its page limit with too little content to scroll leaves nothing that asks for more; that is a known gap, not handled yet.
- Interrupt only for failures, blockers, decisions, and meaningful completion. Say what happened, what the system did, and whether action is needed; open the relevant place when a notification is selected; never repeat what is already on screen.
- Keyboard: visible focus, logical order, predictable shortcuts. Escape closes overlays. Background updates never move, remove, or replace what someone is using.

## The canvas

A workflow's graph is edited on a canvas of its own (`web/src/app/workflows/canvas/`), the one screen in the app that is
a surface rather than a list of rows. What that asks for, once:

- The graph is the page. It fills the panel beside the palette of kinds at its left, and nothing is laid over it but the window of the step being edited: no box, no heading, no band of padding. A step is edited in a centred window, as a card is. Pan and zoom belong to the graph, so
  the page itself never scrolls.
- Where a node sits is what the file holds, and the file holds it because a person put it there. A node opens at the
  place it was saved at, a node dragged is a change to the file like any other, and the read-only view of the same
  workflow draws the same positions by hand (`WorkflowGraph.tsx`) so the two never disagree about where a node is.
- The graph library brings its own look, and none of it reaches the screen: `src/styles/canvas.css` sets the library's
  colour variables to the app's tokens, so the dots of the grid, the wires, a chosen node and its handles are the same
  fills and hairlines as everywhere else, in both themes, and neither of the library's themes is loaded. Its own name in
  the corner is switched off too, since the app says where it is; the license and size of the library are in `web/NOTICE`.
- A node on the canvas is the card of the read-only view, with two things added: the ports it leaves and comes in by,
  each named for the port it is, and what is wrong with it. A refusal is written on the node it names, in the error
  colour, with the wire carrying it coloured the same way, and the same sentence once more above the graph — where a
  person looks after pressing Save, which is where the state of the graph stands: "Saved", "Unsaved changes", "Saved as
  1a3b3c0".
- A run picked is drawn over the graph rather than beside it: the node's own status on its card, and what it was given
  and gave back in the step's window. Nothing opens, nothing navigates: the run is a way of
  reading the same graph.
- Nothing is asked twice, and the keyboard is the graph's own. Delete takes the chosen node or wire away, with the wires
  that hung off it; the start of the graph cannot be taken away, since a workflow without it is not a workflow; and
  leaving with changes that are not saved asks once, with a way to save on the way out.

## Motion
- Update content in place. Never scroll someone away from what they are reading; follow newly arriving content only when they are already at the end. A step someone opens is read where it is: unfolding one lets go of the end, even when the list was sitting on it. Do not animate the same content twice.
- **Fades only, up to 120ms**, ease-out (`--motion-fast` 80ms, `--motion-routine` 120ms; nothing is slower). A menu, a window, the palette, a toast and a tooltip fade in; a row's hover fill changes in the same time. Nothing slides, scales, lifts or springs, and nothing moves that the person did not cause. A card does not lift on hover, and a panel that opens does not push the page across.
- **Reduce motion.** Appearance has a "Reduce motion" switch, which starts from the system's preference (the switch is kept in this browser, `src/motion.ts`, and written as `data-reduce-motion` on the root; the system's own request keeps working without it). It turns every transition off, ends every animation before it is seen, and stops the running ring, which then shows as a still ring (`src/styles/tailwind.css`, outside every layer so no utility outranks it; `e2e/motion.e2e.ts` holds it).
- **No infinite animation** except the running ring, and none on a card that is off screen.
- **Blur only where it is small.** A menu, a tooltip and the command palette may blur what is behind them (`backdrop-filter`, 16px at most) over a `--panel` of 94% (never so thin that the text behind shows through; in the dark theme the layer is the opaque `--panel`). A centred window, a panel and the page header do not: a window sits on a dimmed page, which is not blurred. Appearance has a "Blur on menus" switch, and blur is off by itself where the machine has no GPU rasterisation; the surface is then the opaque `--panel`.
- Activity indicators stay subtle and carry real status text.
- Meaningful images get useful alternative text and room enough to inspect. No decorative symbols that imply intelligence without explaining it.
- Skip decorative gradients, glows and grain, progress without context, unsupported time estimates, unnecessary celebration, and redundant success messages.

## Scrolling performance
The desktop window renders with WebKitGTK, where paint costs show as stutter. So lists and board columns are virtualised; each board column scrolls on its own, and the page does not reflow when a window opens; scrollers carry no mask or fade; nothing but a small menu, a tooltip or the palette blurs; hover changes a colour and moves nothing; and no element runs an animation that nobody is looking at.


## Docs
Shared principles, voice, visual language, interaction patterns, and the shared component contract live in this page. Feature behavior goes in the doc comments of the feature's own modules and in the layout of `web/README.md`, which link here instead of repeating these rules.
