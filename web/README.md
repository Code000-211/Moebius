# Moebius web app

The web app is the desktop app's counterpart in the browser: React, TypeScript and Tailwind, built into `dist/` and served
by the Moebius server at `/`. It talks to the server only through the [desktop API](../docs/reference/desktop-api.md),
as the iced app does, and signs in with a cookie (`POST /v1/session`). The shell is sign-in, sidebar, command bar,
connection banner and live data; every page the sidebar lists has one, and `app/shell/Placeholder.tsx` is left for an
entry of `app/pages.ts` that has no route of its own yet.

## Layout

| Path | What it holds |
|---|---|
| `src/main.tsx` | The entry: takes `?login=` out of the address, sets the colour theme, then starts the router and the query cache. |
| `src/app/` | `App.tsx` (the gate: sign-in, unreachable, shell), `routes.tsx`, `pages.ts` (the sidebar's entries, and `SETTINGS_PAGES`, the pages the sidebar shows once Settings is chosen), `SignIn.tsx` and `shell/` (sidebar, command bar, banner, page frame). |
| `src/app/board/` | The board page: `BoardPage.tsx` (columns, drag and drop, notices), `model.ts` (the logic: columns, filters, status, what a drop means, card words; tested in `model.test.ts`), `Dock.tsx` (the inspector), `decisions.ts` (approve, merge, send back, retry, unblock), `CardMenu.tsx` (the ⋯ menu of a card, on the board and in the dock: Block… or Unblock, and Delete…), `CardDialogs.tsx` (the questions that block and delete ask; tested in `CardMenu.test.tsx` and `CardDialogs.test.tsx`), `FilterBar.tsx`, `NewIssueDialog.tsx` and `ScriptPage.tsx`, which edits a column's script and saves it as a commit; `ColumnMark.tsx` gives each column its status mark; the dock shows the diff of a card's change, and a Terminal button opens a shell in the repository's clone. |
| `src/app/runs/` | The run view (`/runs/:id`): status, agent, the card it links back to, the run's events drawn with the chat's entries and steering (`POST /v1/runs/{id}/steer`), a terminal in its worktree, and its changes. |
| `src/app/terminal/`, `src/app/ports/` | The Terminal page (a shell on the server) and the Ports page (`ports.ts` is the logic, tested in `ports.test.ts`). |
| `src/terminal/` | `Terminal.tsx`, xterm.js on the server's terminal WebSockets (copy on release and Ctrl+Shift+C, Ctrl+Shift+V and right click paste, resize on fit), and `protocol.ts`, the messages (tested). Written for Moebius: Vibe Kanban's `TerminalPanel` only lays out tabs. |
| `src/diff/` | `parse.ts` reads a unified diff (files, hunks, numbered lines, side-by-side rows; tested), `DiffView.tsx` draws it in two columns — the changed files as a tree on the left, in the rows `ui/files/` draws, and the lines of whichever one is chosen on the right, with a Unified / Side by side toggle —, `Delta.tsx` writes what a diff costs as `+21 -2` (the tree, the summary and the dock's headline all use it), `Changes.tsx` reads a run's or a change's diff and loads the view lazily — and `ChangesCard` is the dock's one line about a change, which opens the whole diff in a sheet. |
| `src/app/inbox/` | The Inbox page: `InboxPage.tsx` (filters by kind and repository, and `?change=<id>`, the one change a link from the package sheet lands on), `ItemCard.tsx` (one item: tag, facts, what a proposal changes with the trials it was put through, the issues of a grilling to edit, the buttons), `actions.ts` (the request each button sends) and `model.ts` (what an item is about, its sentence, what a proposal offers, the filters, editing a grilling's issues; tested in `model.test.ts`). |
| `src/app/projects/` | The Projects pages: `ProjectsPage.tsx` (the list, with the question before a delete), `AddRepositoryPage.tsx`, `ProjectPage.tsx` (`/projects/:id`: the trusted logins of a repository on a forge and its files) with `FilesPanel.tsx`, `FileTree.tsx`, `FileEditor.tsx` (a field, saved as a commit; Ctrl+S) and `FindPanels.tsx` (the two search bars always shown above the two columns: "Go to file", Ctrl+P, and "Search in files", Ctrl+Shift+F, both searching as the typing pauses); `model.ts` and `fuzzy.ts` are the logic. |
| `src/app/fleet/` | The Fleet page: a tile for each working run (`FleetPage.tsx`) that follows its events with `chat/useThread`; `model.ts` says which runs are working. |
| `src/app/usage/` | The Usage page: `UsagePage.tsx`, `Heatmap.tsx` of the last year, `Boxes.tsx` (models, time, waits, lens, agents), `figures.ts` (numbers in three figures, dates) and `model.ts`. |
| `src/app/start/`, `src/app/settings/` | Get started (`StartPage.tsx`, the sidebar lists it only while a step is missing) and Settings (`SettingsPage.tsx`: the keys of `moebius.toml` in boxes, `fields.ts` has the keys and what a save sends; `RestartBanner.tsx`, which the Models page can show too; `Webhooks.tsx`; and `Appearance.tsx`, where a person chooses the colour theme for this browser). |
| `src/packages/` | The package sheet, a floating panel like the floating chat for the two package managers: `PackageSheet.tsx` (the frame, the Browse and Installed tabs, what the last request told), `PackageSheetContext.tsx` (which catalog it is pointed at, and whether it shows, in the slot it shares with the floating chat; the shell holds one, and pages call `open("moebius")` from the Plugins page or `open("pi", agent)` from an agent's Extensions), `Browse.tsx` (search and kind filter), `Installed.tsx` (Update, Remove, Update all), `MoebiusDetail.tsx` and `PiDetail.tsx` (a package opened), `Report.tsx` and `useActions.ts` (one request at a time, told in the API's words, linked to the change in the Inbox), `parts.tsx`, and `model.ts` (the catalogs and their kinds, the sentences an install, update or removal is told in, what a refusal says; tested in `model.test.ts`, the sheet's states in `PackageSheet.test.tsx`). The frame, the minimise behaviour and the opener button are `ui/page/Sheet.tsx`, and the slot the two sheets share `ui/page/SheetSlot.tsx`, which `chat/FloatingChat.tsx` uses too. |
| `src/app/agents/`, `skills/`, `mcp/`, `models/`, `plugins/` | The Agents, Skills, MCP servers, Models and Plugins pages (routes in `src/app/routesB.tsx`). The Plugins page lists what is installed from the registries (`InstalledPackages.tsx`) and has "Browse packages"; the Extensions of an agent has "Browse Pi packages", both open the package sheet. Each has its logic in a `model.ts` with a test (what a save sends, or what is wrong with the fields; for Models the form made from `GET /v1/settings` and the plan of a save: keys to store, keys to set and remove). `src/app/harness/` holds what they share: `form.tsx` (box, field, select, chip, list beside editor), `HarnessSetup` (set up the harness repository, then restart), `ImportPanel` and `importRequest.ts` (the text the Moebius agent is asked to import skills or MCP servers; zip upload), `ModelField`. An API key typed on the Models page is a password field, sent once to `PUT /v1/secrets/{name}` and never shown back; the OAuth login opens the provider's page in a new tab and takes the pasted code or address. |
| `src/app/rules/`, `automations/`, `retro/`, `experiments/`, `decision/` | The pages of Rules (`RulesPage`, the rule cards of `RuleList`, `file.ts` puts a rule back into its file's text), Automations (the schedule, chat and firings of the one chosen), Retro (the days, errors, proposals and `Lessons`), Experiments (briefs, baseline and levers) and Decisions (kinds, exports, shadowing and takeovers). Each has the logic in `model.ts`, tested in `model.test.ts`. |
| `src/api/` | The client of the desktop API: `http.ts` (requests and `ApiError`), `calls/` (one module per part of the API, as `crates/moebius-desktop/src/client/calls/`), `types/` (written by hand from the reference), `events.ts` (the WebSocket), `keys.ts` (query keys) and `errorText.ts` (what a failed request says). |
| `src/live/` | The event stream feeding the query cache (`invalidation.ts`; the `package_*` events and `extension_update_available` read the sheet and the Plugins page again) and the connection state for the pages (`LiveProvider.tsx`). |
| `src/auth/login.ts` | The `?login=<code>` link the server prints. |
| `src/chat/` | The Chat page (`/chat`, `/chat/:id`) and the floating chat of the shell: `conversation.ts` folds a run's events into entries (the logic of the desktop app's `Conversation`, with the provider's errors added), `turns.ts` groups them, `follow.ts` decides when the list follows an answer, `readMarkers.ts` and `chatList.ts` keep the unread markers and the list current, `useThread.ts` follows one chat over the event stream (it opens the chat at its last message, reads back until the window of `MESSAGE_WINDOW` entries is full, and reads the rest on `loadOlder`), `drafts.ts` keeps a half-written message where it was left, `Starters.tsx` offers the first message in a chat that has none. |
| `src/ui/` | `CodeEditor.tsx` (CodeMirror with Lua highlighting, loaded lazily by the script editor; written for Moebius). Components copied from Vibe Kanban's `@vibe/ui`, each with a header comment saying where from and what changed. `ui/lib/format.ts` is where a count, a day ("Today", then "Yesterday", then `27 Oct`) and a gap ("5 min ago") are put into words once, so two lists never say the same thing two ways. `ui/chat/` holds the chat entries (messages, tool pills, thinking, todos, files with diffs, errors, the composer box); they are presentational, so the runs page can draw a run's conversation with them. `Markdown`, `CodeBlock`, `CopyButton`, `DiffBody`, `PillSelect`, `Turn` and `Tag` there are written for Moebius, not copied. `components/Mark.tsx` is the Moebius strip, drawn in the current colour, the same path the tab icon carries. `ui/lib/useBoxWatch.ts` is the one watching behind every box that says something about its own content: it measures the box each render and whenever its size changes, and `ScrollFades`, `FadedLine` and `FadedClamp` read off it. `ui/lib/useSettled.ts` is the pause a search waits for after the typing (the files panel and the package sheet use it). |
| `src/ui/page/` | The pieces every page shares, written for Moebius: `PageHeader.tsx` (where the page is, its title, one sentence of what belongs here, its actions; `Body` is the same sentence inside a box), `List.tsx` (`ListCard`, `ListDetail`, `Table` and `CELL`; the rows of a `ListDetail` are a real `<ul role="list">`, so a person hearing the page learns how many there are), `Empty.tsx` (the empty state), `Status.tsx` (a state in a word, carried by its colour), `ScrollFades.tsx` (`ScrollFades` and `useScrollFades`: the mask that fades the edge of a scroller where there is more behind it — a board lane, the sidebar's rail, the ends of a conversation), `FadedLine.tsx` (one line, the end that runs short faded, the whole of it in the tooltip) and `FadedClamp.tsx` (three lines of a description on a card, faded only when the box is measured to have more behind the cut), `Skeleton.tsx` (`Skeleton`, `SkeletonList`, `SkeletonCard`, `SkeletonLines`, `SkeletonBoard` and the `Reading` region around them: what a page draws while it is being read, in the shape of what it will show, with the state spoken rather than drawn), `Tabs.tsx` (`Tabs` and `TabPanel`: the switch between the views of one thing, the panel named by the tab that shows it), `Box.tsx` (a card that names its region from its own heading), `Chip.tsx` (a filter), `Switch.tsx` (one thing on or off, with the word it stands on beside it), `Sheet.tsx` (`SheetFrame`, `SheetOpener` and `useSheetBehaviour`: the floating sheet's frame, its round button and Escape to minimise, which the floating chat and the package sheet share), `SheetSlot.tsx` (which of the two is showing: opening one minimises the other), `Field.tsx` (`Field`, `TextArea` and `Select`) and `Notice.tsx`. The classes a control is built from and the ids of a field's label and hint are in `ui/lib/field.ts`, where a control that names itself is left alone; `Field.tsx` re-exports them as `CONTROL` so `components/Input.tsx` does not reach up into the page pieces. |
| `src/ui/files/` | The tree of paths, drawn the same way wherever a person meets one: `tree.ts` folds a list of paths into the folders and files of `treeOf` (tested — one row for a folder every path shares, folders before files, the whole path kept on every node), and `TreeRows.tsx` holds the `FolderRow` and `FileRow` a tree is made of plus the `indent(depth)` that lines them up. The repository page fetches its tree a folder at a time and a change's diff takes its paths from the diff it already has, so both draw the same rows. |
| `src/styles/` | `theme.css` (the tokens of both themes as CSS variables — the colours, radii, the 8pt spacing grid, the type scale, motion and the layers), `tailwind.css` (the same tokens as Tailwind colours, radii, sizes and shadows, plus the type utilities, the `clamp-*` and `fade-last-*` that stop a description at a number of lines and fade its last one, and the quiet scrollbar every pane shares, painted only under the pointer), `fonts.css`, `highlight.css` (the CodeMirror theme from the same tokens). |
| `e2e/` | The Playwright tests and the server they run against; `e2e/fake/` has the data and routes of a page in the fake server (`board.ts` for the board, `chat.ts` for the chats and what an agent does in them, `pages-a.ts` for the Inbox, Projects, Fleet, Usage, Get started and Settings pages, `pages-b.ts` for the harness configuration, models and plugins, `pages-c.ts` for Rules, Automations, Retro, Experiments, Decisions and the column scripts, `packages.ts` for the Moebius registries and the pi.dev catalog behind the package sheet (`packages.e2e.ts` tests it); `events.ts` is how a read of a run's events answers `after`, `tail` and `before`). `e2e/design/audit.test.ts` is the design audit that `pnpm audit:design` runs. |
| `public/fonts/` | Inter Variable (latin and latin-ext subsets) and JetBrains Mono, both under the SIL Open Font License 1.1. |

## Scripts

Install with `pnpm install` (pnpm 10, Node 22). From this folder:

| Script | What it does |
|---|---|
| `pnpm dev` | Vite's dev server with hot reload. It proxies `/v1`, the event WebSocket included, to `MOEBIUS_URL` (default `http://127.0.0.1:7171`). |
| `pnpm build` | Type-checks and builds into `dist/`, which the server embeds. |
| `pnpm check` | `tsc --noEmit` and Biome (lint, format and import order). `pnpm format` fixes what Biome can. |
| `pnpm test` | Vitest on `src/**/*.test.ts(x)` in jsdom: the client's errors, the events-to-cache logic, the `?login` handling, the sign-in page, the board's logic (filters, grouping, drop rules), the `model.ts` of each page, the chat's folding of a run's events, the diff parser, the terminal's protocol, and `styles/tokens.test.ts`, which keeps `theme.css` and `tailwind.css` inside the standard: the names of both themes, their values, radii, spacing, type and contrast, no colour literal outside `theme.css`, and a Tailwind name for every colour token. |
| `pnpm audit:design` | Renders every page in a headless Chromium against the fake server, in each theme at each of two widths (1440 and 900), and reports what breaks the standard: content wider than its box, clipped text, a radius, size or weight off the scale, spacing off the 4px grid, a card or control that still draws a line around itself, a control under 40px, text under contrast, a row not aligned with its siblings. It writes a `report.md`, a `report.json` and a screenshot per page under the folder named by `MOEBIUS_DESIGN_OUT` (a temporary folder when that is unset). `MOEBIUS_DESIGN_STRICT=1` fails the run on the three findings that are always wrong rather than a matter of taste — content wider than its box, clipped text, and words below their row — and only in the 1440 render, where a box has the width the standard assumes; the other findings are reported in every render and judged by reading the report. `MOEBIUS_DESIGN_ONLY=chat-chats` measures one route alone, which is what makes a before-and-after cheap enough to afford both. The audit is not in CI and is run by hand before a change that can move a pixel is merged. To point it at a server of your own instead of the fake one, set `MOEBIUS_DESIGN_URL` and `MOEBIUS_DESIGN_TOKEN` together (the label it writes is `MOEBIUS_DESIGN_LABEL`), and `MOEBIUS_E2E_CHROMIUM` to name the Chromium to launch. |
| `pnpm e2e` | Playwright in Chromium, driven by Vitest. Needs `pnpm build` first. |

## Run it against a server

```sh
# a server (see the README at the top): it prints http://127.0.0.1:7171/?login=<code>
cargo run -p moebius -- serve
cd web && pnpm install && pnpm dev
```

Open the address `pnpm dev` prints. Sign in by pasting the content of the server's token file (`.moebius/token`), or open the
`?login=` link on the dev server's address instead of the server's. In production there is nothing to run: the server
serves the built app at `/`.

The server must answer `index.html` for the app's paths (`/inbox`, `/board/3`, ...), so that a page opens and reloads by its address.

## The end-to-end tests

`pnpm e2e` starts a small fake of the server (`e2e/fakeServer.ts`, Node `http` and `ws` on port 0) that keeps to the session
contract, answers the few routes the shell reads and serves `dist/`. The tests sign in with the token and with the
`?login=` link (the code leaves the address), open pages from the sidebar and the command bar, and cut the server to see
the banner. They wait on what the page shows, never on time.

With `MOEBIUS_E2E_SERVER=<path to the moebius binary>` the tests start a real `moebius serve` instead, in a new directory
on a free port, and read the login link it prints. That needs a server that has the browser sign-in and serves the app.
Tests that need to stop and start the server again, or that rely on the fake's data (a board named `demo`), are skipped
against the real one.

Chromium comes from Playwright: `pnpm exec playwright install chromium` (add `--with-deps` on a bare Linux). Set
`MOEBIUS_E2E_CHROMIUM` to the path of a Chromium to use one that is already there.

## Theme

The web app follows [`docs/design-web.md`](../docs/design-web.md), which is not [`docs/design.md`](../docs/design.md):
that one is the brief
the iced desktop app is built to, and `crates/moebius-desktop/src/theme.rs` is still the source of truth for it. Here the
standard is a light and dark pair of palettes on a warm near-black and near-white canvas, borderless surfaces raised by
a shadow rather than outlined by a line, generous radii (16 for a card, 12 for a control, full for a pill), one readable
sans at 400 for prose and 600 for labels, and spacing on an 8pt grid, where 4 is the half-step a tight row needs and
2 a nudge. The numbers are what count: `tokens.test.ts` and the design audit below enforce them, so read those for
anything this paragraph leaves out.

`src/styles/theme.css` holds every token twice, once for dark (the default, on `:root`) and once for light (under
`:root[data-theme="light"]`); `src/styles/tailwind.css` makes them Tailwind colours, radii, sizes, shadows and type
utilities; `src/theme.ts` is the `system` / `light` / `dark` choice, saved under `moebius.theme`, applied before the
first paint by the script in `index.html`, and offered by the Appearance box on the Settings page. The names of the desktop
palette (`text-high`, `bg-panel`, `border-line`, `text-accent` and the rest) are gone, aliases and all: every component,
copied from Vibe Kanban or written here, names a token, and a colour name that is not one builds to nothing, because
`tailwind.css` clears Tailwind's own palette first. `src/styles/tokens.test.ts` fails when the two themes do not define
the same names, when a token leaves the values of the standard, when any text colour falls under 4.5:1 in either theme,
when a component writes a colour as a literal instead of naming a token, or when a token has no Tailwind name. Change
the tokens there, never in a component.

## What changed from Vibe Kanban's components

The components of `src/ui/` were written for React 18 and Tailwind 3. Here: `forwardRef` is gone in `Button` and `Input`
(React 19 passes `ref` as a prop) and `React.ElementRef` is `React.ComponentRef`; Tailwind's `tailwindcss-animate` is
`tw-animate-css`; the colour names the components used (`text-high`, `bg-panel`, `text-brand`, ...) are gone, and every
copied component was rebuilt on the tokens of `theme.css`, borderless at the radii of the standard, carrying the shadow
of its layer instead of an outline; `react-i18next`, Tauri helpers and `lucide-react` are
gone (the icons are Phosphor's, the text plain English); `cn` merges Tailwind classes itself. Nothing that reports to
a service (PostHog, Sentry) is copied. Vibe Kanban's `CommandBar` is not copied: it is made of issues, branches and priorities
that Moebius does not have, so `app/shell/CommandBar.tsx` builds the page switcher from its `Command` primitives. The
board copies `KanbanBoard`, `KanbanCardContent`, `KanbanIssuePanel`, `PrBadge` and `RunningDots`; its `KanbanBadge` is
gone, because a state in a word is the shared `ui/page/Status` pill on every page. `KanbanBoard` drags with
`@dnd-kit/core` instead of `@hello-pangea/dnd` (pointer and keyboard; the keyboard moves to the next column even when it is scrolled out of view, a drag overlay shows the card, announcements name card and column); it loses its
Gantt types, its "add task" button and i18n. `KanbanCardContent` shows what a Moebius card has (number as a link to the issue, agent and
round, round bar, state badges, pull request) in place of priority, assignees, tags and relationships. `KanbanIssuePanel`
is a read-only inspector that keeps the panel's frame (header, close, Escape, bordered sections) and takes its sections as
children; Vibe Kanban's one is the form for editing an issue. Vibe Kanban's `KanbanFilterBar` is not copied (its
filters are priorities, assignees and tags): `app/board/FilterBar.tsx` is native selects. The chat components (`src/ui/chat/`) are copied the same way: `ChatToolSummary`, `ChatAggregatedToolEntries`, `ChatCollapsedThinking`, `ChatTodoList`, `ChatFileEntry`, `ChatErrorMessage`, `ChatEntryContainer`, `ChatUserMessage`, `ChatAssistantMessage`, `ChatMarkdown`, `ChatBoxBase` and `ToolStatusDot`, without i18n and the Lexical editor (the composer is a plain auto-growing textarea). `ModelSelectorPopover` is not copied: it is made of providers and accordions, and the models of Moebius are one list, so `PillSelect` is a small menu with a filter. Later issues copy more components (terminal) the same way.

## Licenses

The app is Apache-2.0 like the rest of Moebius. `NOTICE` and `LICENSE-APACHE-2.0` are for the Vibe Kanban components. The
fonts are under the SIL Open Font License 1.1 (`public/fonts/*/OFL.txt`).

Direct dependencies, all checked with `pnpm licenses list`; what goes into `dist/` is MIT, except `class-variance-authority`
(Apache-2.0), `highlight.js` (BSD-3-Clause), a few ISC packages of the Markdown tree and `tslib` (0BSD, through Radix):

| Package | License | Used for |
|---|---|---|
| `react`, `react-dom` | MIT | The UI |
| `react-router` | MIT | Routing (lighter than TanStack Router's generated route tree for this many flat routes) |
| `@tanstack/react-query` | MIT | Server data and the cache the events feed |
| `@radix-ui/react-dialog`, `-dropdown-menu`, `-tooltip`, `-slot` | MIT | Behaviour of the copied components |
| `@dnd-kit/core` | MIT | Dragging cards on the board, with pointer and keyboard (it brings `@dnd-kit/utilities`, `@dnd-kit/accessibility` and `tslib`, MIT and 0BSD) |
| `cmdk` | MIT | The command bar |
| `@phosphor-icons/react` | MIT | Icons |
| `react-markdown`, `remark-gfm` | MIT | The agent's text as GitHub flavoured Markdown (the unified/remark/rehype packages under them are MIT, with a few ISC) |
| `rehype-highlight` | MIT | Highlights code blocks with `lowlight` (MIT) and `highlight.js` (BSD-3-Clause), its common languages |
| `react-virtuoso` | MIT | The virtualised list of a long conversation |
| `@types/hast` | MIT | Types of the Markdown syntax tree (development) |
| `class-variance-authority` | Apache-2.0 | Button variants |
| `clsx`, `tailwind-merge`, `tw-animate-css` | MIT | Class names and animation utilities |
| `vite`, `@vitejs/plugin-react` | MIT | Build |
| `tailwindcss`, `@tailwindcss/vite` | MIT | Styles (its CSS engine `lightningcss` is MPL-2.0 and only runs at build time) |
| `typescript` | Apache-2.0 | Types |
| `@biomejs/biome` | MIT OR Apache-2.0 | Lint and format |
| `vitest`, `jsdom`, `@testing-library/*` | MIT | Unit tests |
| `playwright` | Apache-2.0 | End-to-end tests (version pinned to the one preinstalled in the development image) |
| `ws` | MIT | The fake server's WebSocket |
| `@codemirror/state`, `-view`, `-language`, `-commands`, `-lint`, `-legacy-modes`, `@lezer/highlight` | MIT | The script editor (`ui/CodeEditor.tsx`): editing, Lua highlighting, and the marker of the line a refused script names. It is its own chunk, loaded when the script page opens |

Other transitive licenses in the tree: ISC, BSD-2/3-Clause, MIT-0, BlueOak-1.0.0 and CC0-1.0, all for development tools.

## Later improvements

- The API types in `src/api/types/` are written by hand from `docs/reference/desktop-api.md`. Generating them from the Rust
  types would remove the chance of drift; that is out of scope here.
- The search over cards waits for a search of the whole app.
- The board has no route to move a card to a column: a person decides what waits in the inbox, so a drop means approve
  (Backlog to Not started), merge (to Done), send back (to In progress, with a reason) or retry (a stuck card, to In progress), and
  any other drop is refused with the reason. The desktop app's "let the Moebius agent write the issue" waits for the chat page.

## Terminals, diffs and ports

The terminals are xterm.js over the WebSockets of `GET /v1/runs/{id}/terminal`, `/v1/chats/{id}/tui` (the Terminal button
of the chat's top bar, while the chat waits), `/v1/terminal` (the Terminal page) and `/v1/repositories/{id}/terminal` (the
board's Terminal button). A browser is not told why a WebSocket was refused, so a shell that does not open says that only a
person can open one and what else may be the matter. The diffs come from `GET /v1/runs/{id}/diff` and
`GET /v1/changes/{id}/diff`, routes added with this page. The diff view is an own renderer on the unified diff: a
full-text merge view needs the whole old and new file, and the server gives the hunks. Ports: the tunnel
(`GET /v1/ports/{port}/tunnel`) is a WebSocket of raw bytes that only the desktop app can turn into a local port, so a
browser tab cannot open it; the Ports page lists the ports with their owner and the `ssh -L` command that forwards one.

New dependencies, all MIT: `@xterm/xterm` 6.0.0, `@xterm/addon-fit` 0.11.0, `@xterm/addon-web-links` 0.12.0.
