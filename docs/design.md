# Desktop app design

The desktop app is a dense developer tool in a dark theme, Deep Velvet: a violet-black ground with islands of surface,
soft radii, a compact type scale, and colour only where it says something about a run or is the one thing to press. Every value below is a named constant in
[`crates/moebius-desktop/src/theme.rs`](../crates/moebius-desktop/src/theme.rs), which is the single source of truth;
this page says what the constants are for.

The mockups in [`docs/design/`](design/) (HTML and PNG: [board](design/board.png), [live view](design/live-view.png),
[inbox](design/inbox.png), [agents](design/agents.png) and [usage](design/usage.png)) are the old look. They are kept
for layout only: where the screens sit on the page, what each one holds and in which order. Their colours, sizes,
radii and type are not the app's. Where a mockup and the rules below disagree, the rules win. The chat screen has no
mockup; it follows the live view's conversation and these rules.

## No literals in screens

A screen or widget has no colour, size, radius, padding, spacing, border width or fixed width of its own. Everything
comes from `theme`: a value two places need is a constant there, and a new value is added there first, never repeated
in a screen. The fonts are `theme::MANROPE`, `theme::manrope(weight)` and `theme::MONO`; no generic family is named.
The shared pieces (`widgets/page.rs`, `widgets/form.rs`, `widgets/list_editor.rs`, `widgets/inline_edit.rs`) are used
for headers, boxes, buttons, tags, chips, labelled inputs and list-and-editor pages instead of building them again.

## Colour

A dark violet base carries the whole app. Coral is the accent; lavender, mint, amber and red are status colours and
nothing else.

| Role | Constant | Value | Used for |
|---|---|---|---|
| Ground | `GROUND` | `#0F0D14` | Behind everything |
| Surface | `SURFACE` | `#181520` | Islands: boxes, the sidebar, inputs |
| Lane | `LANE` | `#120F18` | Board columns, recessed editors, tool calls |
| Card | `CARD` (hover `CARD_HOVER`) | `#211C2B` | Cards on the board and in lists |
| Recessed | `RECESSED` | `#0B0910` | Wells: tags, rows of the board's dock |
| Line | `LINE` / `LINE_STRONG` | `#2A2438` / `#3B334D` | Borders |
| Text | `TEXT` / `TEXT_SECONDARY` / `TEXT_MUTED` | `#F7F4FC` / `#AAA0BA` / `#685E78` | Primary / secondary / muted |
| Accent | `ACCENT` (fill `ACCENT_FILL`, line `ACCENT_BORDER`) | `#F27663` | The primary button, the chosen item, links back |
| Live | `LIVE` (fill `LIVE_FILL`, line `LIVE_BORDER`) | `#C084FC` | Runs working now, their progress |
| Runs on its own | `TEAL` (fill `TEAL_FILL`) | `#6EE7B7` | Passing checks, online, done by itself |
| Needs you | `AMBER` (fill `AMBER_FILL`) | `#FBBF24` | Ready to merge, the inbox count, a restart banner |
| Blocked | `RED` / `RED_TEXT` (fill `RED_FILL`) | `#FB7185` | A broken rule, a stuck or blocked card, a refusal; only these |

- The primary button is coral (`ACCENT` with `ON_FILL` text), one to a view. Every other button is a surface with a line.
- The chosen item is coral (`ACCENT`, fill `ACCENT_FILL`); a card grown into its editor has a 1.5 px line
  (`BORDER_FOCUS`).
- Tags are pills with a border, a fill and a text colour per role (`Tone`: plain, live, teal, amber, red); a live or red
  one has a dot.
- Charts use the validated categorical order in `CHART` (coral, mint, lavender, amber). A model keeps its colour on every
  chart.
- Code is highlighted with the dark `HIGHLIGHT` theme.
- The sidebar is an island of the surface colour (`SIDEBAR` and its text, group, current and hover colours).

## Type

Manrope for headings and prose; JetBrains Mono for everything that is metadata: breadcrumbs, tags, asides, status and
notice lines, list subtitles, counts, and all code (diffs, terminal output, tool names, paths, commands, identifiers,
the instructions editor). Numbers use tabular figures where they line up.

| Role | Constant | Size |
|---|---|---|
| Page title | `PAGE_TITLE` | 19 bold |
| Box heading | `BOX_HEADING` | 14 semibold |
| Card title | `CARD_TITLE` | 13.5 semibold |
| Body | `BODY` | 13 |
| Context line, buttons | `CONTEXT` | 12.5 |
| Breadcrumb | `BREADCRUMB` | 12 |
| Label, tag, aside | `LABEL` | 11 |
| Code | `CODE` / `CODE_SMALL` | 12 / 11.5 |

Sentence case for every label, button, tag and heading. Names and code keep their own case (`radiance`, `code.luau`).

## Shape and spacing

- Radii are soft: islands 14 px (`RADIUS_BOX`), cards 10, controls and items 8, small buttons 6; tags are pills
  (`RADIUS_ROUND`). Shadows are soft and deep (`FLOAT_SHADOW`) and only under what floats.
- A page has a padding of 14 and 18 (`PAGE_PADDING`) and a gap of 12 between its parts (`PAGE_GAP`). A box has a padding
  of 14 (`BOX_PADDING`) and a heading, and its content is 8 apart.
- Controls are compact: an input, a dropdown or a button has a padding of 6 and 12 (`CONTROL_PADDING`). Smaller gaps
  come from `GAP_XS`, `GAP_S`, `GAP_M` and `GAP_L` (4, 6, 8, 12). A text editor pads 10 (`EDITOR_PADDING`) and is short,
  tall or full height (`EDITOR_SHORT`, `EDITOR_TALL`, `EDITOR_FULL`).
- Fixed widths are constants too: the sidebar (`SIDEBAR_WIDTH`, 220) and its rail (`RAIL_WIDTH`, 56), the list beside an
  editor, the label column of a table of facts, the card of the connect screen.

## Copy

Short and technical, in the voice of a tool: "Committed: abc123", "No changes to commit.", "Reading the skill…",
"Read only: this server has no rules file." A line says what is, or what to do, in as few words as it can; it does not
explain what the screen is for. Errors name the cause. Notice, status and error lines are in the code font; an error
is red, a note is secondary grey.

## Connecting over SSH

The connect screen has a "Connect over SSH" button under Connect. It turns the card into a list of hosts (the hosts of
`~/.ssh/config`, of the file named in the settings, and the servers connected to before), each row the name in semibold
and `user@host:port` in the code font; under the list a "Add new SSH host" (a field for `user@host` or an `ssh`
command and an "Add host" button) and "Connect by address". A pick opens the steps: a tag column (`done` teal, `running`
plain, `failed` red) and the step's name, the question of `ssh` in the code font with a hidden field and Answer and
Cancel, a "Show log" button that opens the log in a recessed box, and Cancel (Try again and Back after a failure). A host
without Moebius shows what is missing, the release's URL and two buttons: "Install on the host" (primary) and "Download
here and upload". In the sidebar, when the connection is over SSH or more than one server is remembered, a bordered
button under the app name holds the server's name in the code font and the state under it with its dot ("Reconnecting…" in
amber while the tunnel is down); it opens the list of servers and "Add a server…".

## Security

The server never has to listen beyond `127.0.0.1`: over SSH the only way to it is the tunnel, and the token is read over the
same login and held in memory, not saved. `ssh` does the login; the app only answers its prompts, over the loopback and
with a secret that only the processes it starts know, and keeps no password.

## Layout

- A board's columns name their column and count on the first line and their script with its pencil on the second. Under
  the columns the inspector dock (`DOCK_HEIGHT` high, never scrolling) shows one card: the first working one, else the
  first that waits for you, highlighted in coral on the board. Its left part (`DOCK_META_WIDTH`) holds the card's number,
  title and "Open live view"; its right part one row each for the run, the guard and the card's state.
- An agent's editor puts its settings on top and, under them, two columns: the instructions and the Retro's proposals on
  the left, and skills, MCP servers and extensions on the right.
- A sidebar on every screen, in groups divided by lines: Inbox, Chat, Fleet and Automations; Projects (the heading opens
  the Projects page, every repository with its board and its removal after asking, one entry per repository and a "+" to
  add one); Build (agents, skills, MCP servers, rules); Improve (Retro, experiments, decision model, usage); Setup
  (models, plugins, terminal, ports, settings); tokens used today at the foot. It collapses to a rail of icons. An entry with
  something new (the inbox, an unread chat) has its icon in amber and its count in amber text beside it, without a pill.
- The Ports page (Setup) is one box of rows: the port on the server in the code font with the process and what it
  belongs to under it, the local address with a teal "forwarded" tag (red "failed" with the reason under it), and four
  buttons (Open in browser, Copy address, Change local port, Stop forwarding); a port that is not forwarded has "Forward".
  "Forward a port…" in the header opens a field above the box. A "Ports" button in the header of the live view, a chat
  and the Terminal page opens the page on that run's ports, under a "Back" link. A port that opens in what is watched is
  offered in a box over the page's top: "Open in browser", "Show ports", "Dismiss".
- Every page opens with the same header (`page::header`): an optional breadcrumb in the code font, the title with an
  optional tag, one line of context, and its actions on the right.
- Navigation keeps a way back. A click on a page that takes the person to another place (a card, a column's pencil,
  "Open in Chat", a tile of the Fleet, an item of the inbox, a search result, a link to another page) keeps the place
  left: its page, the tab of its project, and the card, script, chat or agent open on it. The place reached shows
  "← Back to <the place left>" above its header, in secondary grey semibold at the breadcrumb's size (the repository's
  name for a board, "main.rs of radiance" for a tab, "#48" for a card, "the Ready script" for a column's script,
  otherwise the page's title); a click restores the place left as it was. Alt+Left (where no text field takes it) and
  the mouse's back button go back too, and so does Escape in an editor (a column's script, a project's editor tab, the
  rules, an agent) when nothing in it is unsaved; with unsaved text Escape does nothing, so nothing typed is lost. A
  click that leads to the place kept last (the breadcrumb of a card back to its board) is a step back, not a new one. A
  pick of the sidebar starts again: what was kept is forgotten, and the page it opens has no link back.
- A project's page has tabs under its header, the chosen one bold with a dark line under it: Board (the columns and
  cards; a card's page, a column's script and a new issue open from it) and any number of tabs the person opened, each
  with a cross that closes it (Board has none): an editor per file (the file's name in the code font, its path on
  hover, an amber dot while it has unsaved changes; closing an unsaved file asks first), terminals and agent chats
  (numbered when there are several). Beside the tabs stands a small `+`; it drops down a menu over the page (`DROPDOWN_WIDTH`
  wide, a box with the floating shadow, one row per item, closed by a click beside it): Editor (opens "Go to
  file"), Terminal, Agent chat, and Browser, drawn muted with "later" beside it because it is not built. At the right end of the strip a panel button folds the Files sidebar away and brings it back.
- The Files sidebar stands on the right of every tab of a project, `FILES_WIDTH` (260) wide, in a box with the
  line "Files · default branch" in the code font: the repository's default branch as a tree, directories first, each
  with a chevron that opens it (read the first time it opens), files with their size. A click on a file opens it in an
  editor tab at once, or shows the tab it is open in. The editor shows the path, the language, the file in the code
  font highlighted by its extension, and a status line with the cursor's line and column and what the last save did
  ("Unsaved changes.", "Committed to the default branch." or the server's refusal in red). Save, the page's primary
  button, and Ctrl+S (Cmd+S) commit the file on the default branch. A binary file or one larger than 1 MB is named with
  its size and not edited. "Go to file" (Ctrl+P) and "Search in files" (Ctrl+Shift+F) open under the tabs, over every
  tab, and open editor tabs. A terminal tab is the Terminal page's pane (a dark shell as big as the tab) in the
  repository's clone on the server; its shell lives while the project's page is shown. An agent chat tab is a chat of
  the Chat page drawn inside the project, a new one in this project (the Place picker starts at the project); the same
  chat is in the Chat page's list. The open tabs are kept for each project while the app runs.
- Fleet shows every run that is working at a glance: a grid of two or three tiles by width, one for each card whose
  latest run is starting, running or waiting and each chat that is. A tile has the agent, a tag for what it does
  (`code`, `review`, `chat`), the card (`#number title`) or the chat title, the repository, how long the run has run,
  the card's tokens when known, and the last four steps of the run in the compact steps box. A click opens the card's
  live view or the chat. Without a working run the page says "Nothing is working. Runs and chats appear here while
  they work." The sidebar entry says how many are working.
- The Chat page is one column on the canvas (`CH3-D`), `CHAT_COLUMN` (780) wide and centred. On top a floating pill
  bar: "Chats (n) ▾", which drops down the list of chats (newest first, the chosen one marked, `CHAT_LIST_WIDTH` wide),
  an amber tag when chats ask the person something, the chat's title with a pill of its agent and place and its status
  tag, its actions (Ports, Offer changes, End, Stop) and "+ New". Under it the conversation as turn cards: each message
  of the person heads an island on the card colour, and what the agent did and answered until the next one is its
  body. At the foot the change the chat offered, then the composer island (`RADIUS_COMPOSER`, a coral line): the field
  to write in without a frame of its own, and under it the chat's pills on the left and Terminal and Send (the primary
  button) on the right, with a line under the island that says what Send does. "+ New" opens the empty chat at once,
  with no form: the bar with "New chat" and a pill of who it is with and where, a card with a hint, and the composer
  whose pills are Agent, Place (No repository, each project, and the harness configuration when the server has one),
  Model (the models the upstreams serve, a model they do not list shown as it is) and Reasoning (the levels Pi-Bolt
  takes), the agent's model and reasoning until others are chosen; a pill names what it chooses while the pointer is
  on it. Terminal starts the chat without a message and shows it in its terminal as soon as it waits. In a started chat
  the Model and Reasoning pills change the chat while it waits; while its agent works they only show what it runs with,
  muted.
- The floating chat: a small dark "Agent" button (the chat icon and the word, in the code font) stands in the bottom
  right corner of every screen but the Chat page (whose Send stands there), `FLOAT_MARGIN` (16) from the edges, over the
  page. A click grows it into a window of 400 by at most 560 px anchored there, with the box radius and a tight
  shadow: a head with "Moebius agent", a line saying where the chat works ("Works in radiance" or "No repository"), a
  minimise button that turns it back into the button, and, once a chat is going, "New conversation" and "Open in
  Chat"; under it the conversation, drawn as on the Chat page with the steps box, and the field with Send. The chat
  stays across pages. Each message carries a few lines on where the person is (the page, the repository, the open card,
  run or chat, the script, rules or agent being edited with its text cut to 2 500 characters; in a project the open directories of the Files
  sidebar, in an editor tab the file, the cursor's line and column, the selected text and the 40 lines around
  the cursor, cut to about 3 000 characters), which only the agent is told; the chat shows what the person wrote. The
  chats the window shows (those it starts, and those a page's "Ask the agent" hands it) are the window's: the person
  reads them there, so the Chat page does not list them and the sidebar does not count them unread, unless "Open in
  Chat" shows one there on purpose. The Fleet page shows them while their agent works, like any working agent.
  The first message starts a chat with the Moebius agent, in the repository of a project's page and otherwise in none.
  The "Ask the agent" actions (a new issue, an agent's instructions, the rules) open their chat in this window, and the
  speech bubble of the script editor opens it with the start of a message about the script. The Chat page keeps every
  chat and the terminal mode.
- Lists with an editor (Agents, Skills, MCP servers): the cards of the list on the left, each with its name, tags and a
  subtitle in the code font (red when something is wrong), the chosen card with a dark line and the `HEAD_PILL` fill;
  on the right the editor's head with the name and its Delete and Save, then boxes. A delete asks first, in
  place, with Keep and the delete button. A save to the harness configuration repository ends in "Committed: <commit>"
  or "No changes to commit."
- Editing in place: whatever a person or the Moebius agent wrote and the app shows can be changed where it is shown. A
  muted small pencil stands next to it (a card's title, a column's script, a rule, a repository's trusted authors, a
  draft's brief); a click puts the editor in its place: the fields (a text input for a line, a text editor on the
  lane's grey for more), then Save, the primary button, and Cancel, and under them the server's refusal in red, which
  stays until the next save. A save that went through closes the editor and the page shows the text as the server has
  it. Every editor in place is drawn with `inline_edit.rs`.
- Rules are cards: each `## name` section of a rules file is a card with its name in the code font, its text and
  its refusals this week, and a pencil next to the name. The pencil grows that card in place, its line turning dark and
  2 px wide, into the editor of the name and the text with Save, Cancel and, at the right, "Delete rule", which asks
  "Delete this rule?" first. Under the last rule an "Add a rule" card on the lane's grey grows into the same editor. The
  server's rules and each repository's have their cards; "Let the Moebius agent write them" is a button next to
  the heading of the server's rules.
- Setup pages (Models, Plugins, Settings) are boxes of labelled inputs and tables under the header; a banner in
  the amber fill says that the server has to restart, with the restart as its primary button. A key is never shown,
  only whether one is stored. "Log in to a provider" sits beside "Add upstream": a panel in the box lists the built-in
  providers (title, one line, a "Log in" button) and "Other", whose form is the labelled inputs of an `oauth`
  table; the new upstream's row then carries the amber warning, "I understand, log in", and while the login waits a
  mono input "Paste the code or the address you ended on" with the primary "Finish login".
- Budgets and usage are counted in tokens, never in money.
