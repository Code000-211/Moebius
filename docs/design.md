# Desktop app design

The look the desktop app is built to, as agreed on the mockup
([Moebius Desktop](https://claude.ai/artifact/4mxF2AbS9tNP39BJyRNPGJ): board, live view, inbox, agents and usage).
The agreed screens are kept in [`docs/design/`](design/) as HTML (open in a browser) and PNG (1440 × 900):
[board](design/board.png), [live view](design/live-view.png), [inbox](design/inbox.png), [agents](design/agents.png) and
[usage](design/usage.png). Their numbers are made up; the app shows the server's. Where a screen and the rules below
disagree, the rules win. The chat screen has no mockup; it follows the live view's conversation and these rules.

## Type

- **Manrope** for everything that is not code: navigation, headings, text, labels, tags, numbers. Numbers use tabular
  figures so that they line up.
- **JetBrains Mono** only for code: diffs, terminal output, tool names and their arguments, paths, commands, branch names,
  identifiers in running text and the instructions editor.
- Sizes: page title 24 px bold; box heading 16 px bold; card title 15 px semibold; body 14.5–15 px; labels and tags 12 px.
- Sentence case for every label, button, tag and heading. Names and code keep their own case (`radiance`, `code.luau`).

## Colour

| Role | Value | Used for |
|---|---|---|
| Ground | `#EDF2F7` | Behind everything |
| Surface | `#FFFFFF` | Boxes, cards, inbox items |
| Lane | `#F7F9FC` | Board columns, tool calls in the live view |
| Line | `#CBD5E1` | Borders; dashed for separators inside a box |
| Text | `#0F172A` / `#475569` / `#64748B` | Primary / secondary / muted |
| Runs on its own | `#0F766E` (fill `#E6F5F3`) | Live runs, passing checks, progress |
| Needs you | `#B45309` (fill `#FDF3E6`) | Ready to merge, inbox count, the primary button |
| Blocked | `#D92D20` / `#9B2C22` (fill `#FEF3F2`) | A broken rule, a stuck or blocked card; only these |

Charts use the validated categorical order: `#2a78d6`, `#eb6834`, `#1baf7a`, `#eda100`. A model keeps its colour on every
chart.

## Shape

- Rounded corners: columns and boxes 16 px, cards and tool calls 12 px, buttons and inputs 10 px, tags fully round.
- One primary button per view, in the "needs you" colour; everything else is a white button with a border.

## Layout

- A sidebar on every screen, in groups divided by lines: Inbox, Chat and Fleet; Projects (one per repository); Build (agents, skills, MCP servers, rules);
  Improve (Retro, experiments, decision model, usage); Setup (models, plugins, terminal, settings); tokens used today at the foot.
  It collapses to a 68 px rail of icons. Icons are 20 px; an entry with something new (the inbox, an unread chat) has
  its icon in amber and its count in amber text beside it, without a pill.
- Every page opens with the same header: an optional breadcrumb, the title, one line of context, and its actions on the
  right.
- Navigation keeps a way back. A click on a page that takes the person to another place (a card, a column's pencil,
  "Open in Chat", a tile of the Fleet, an item of the inbox, a search result, a link to another page) keeps the place
  left: its page, the tab of its project, and the card, script, chat or agent open on it. The place reached shows
  "← Back to <the place left>" above its header, in teal semibold at the breadcrumb's size (the repository's name for
  a board, "Files of radiance" for a tab, "#48" for a card, "the Ready script" for a column's script, otherwise the
  page's title); a click restores the place left as it was. Alt+Left (where no text field takes it) and the mouse's back
  button go back too, and so does Escape in an editor (a column's script, the project's Editor, the rules, an agent)
  when nothing in it is unsaved; with unsaved text Escape does nothing, so nothing typed is lost. A click that leads to
  the place kept last (the breadcrumb of a card back to its board) is a step back, not a new one. A pick of the sidebar
  starts again: what was kept is forgotten, and the page it opens has no link back.
- A project's page has three tabs under its header, the chosen one bold with a 2 px teal line under it: Board (the
  columns and cards; a card's page, a column's script and a new issue open from it), Files and Editor. Files is the
  repository's default branch as a tree in a white box: directories first, each with a chevron that opens it (read the
  first time it opens), files with their size; a click on a file opens it in the Editor. The Editor keeps the open
  files as tabs, each with the file's name in the code font, an amber dot while it has unsaved changes and a cross that
  closes it (an unsaved file asks first); under them the path, the language, the file in the code font highlighted by
  its extension, and a status line with the cursor's line and column and what the last save did ("Unsaved changes.",
  "Saved as a commit on the default branch." or the server's refusal in red). Save, the page's primary button, and
  Ctrl+S (Cmd+S) commit the file on the default branch. A binary file or one larger than 1 MB is named with its size
  and not edited.
- Fleet shows every run that is working at a glance: a grid of two or three tiles by width, one for each card whose
  latest run is starting, running or waiting and each chat that is. A tile has the agent, a tag for what it does
  (`code`, `review`, `chat`), the card (`#number title`) or the chat title, the repository, how long the run has run,
  the card's tokens when known, and the last four steps of the run in the compact steps box. A click opens the card's
  live view or the chat. Without a working run the page says "Nothing is working. Runs and chats appear here while
  they work." The sidebar entry says how many are working.
- The floating chat: a round teal button (56 px, with the chat icon and a soft shadow) stands 20 px from the bottom
  right corner of every screen but the Chat page (whose Send stands there), over the page,
  like a website's support button. A click grows it into a window of
  420 by at most 600 px anchored there, white with round corners and the same shadow: a head with "Moebius agent", a
  line saying where the chat works ("Works in radiance" or "No repository"), a minimise button that turns it back into
  the button, and, once a chat is going, "New conversation" and "Open in Chat"; under it the conversation, drawn as on
  the Chat page with the steps box, and the field with Send. The chat stays across pages. Each message carries a few
  lines on where the person is (the page, the repository, the open card, run or chat, the script, rules or agent being
  edited with its text cut to 2 500 characters; in a project's Files the open directories, in its Editor the file,
  the cursor's line and column, the selected text and the 40 lines around the cursor, cut to about 3 000 characters),
  which only the agent is told; the chat shows what the person wrote. The chats the window shows (those it starts,
  and those a page's "Ask the agent" hands it) are the window's: the person reads them there, so the Chat page does
  not list them and the sidebar does not count them unread, unless "Open in Chat" shows one there on purpose. The
  Fleet page shows them while their agent works, like any working agent.
  The first message starts a chat with the Moebius agent, in the repository of a project's page and otherwise in none.
  The "Ask the agent" actions (a new issue, an agent's instructions, the rules) open their chat in this window, and the
  speech bubble of the script editor opens it with the start of a message about the script. The Chat page keeps every
  chat and the terminal mode.
- Editing in place: whatever a person or the Moebius agent wrote and the app shows can be changed where it is shown. A
  muted 13 px pencil stands next to it (a card's title, a column's script, a rule, a repository's trusted authors, a
  draft's brief); a click puts the editor in its place: the fields (a text input for a line, a text editor on the lane's
  grey for more), then Save, the primary button, and Cancel, and under them the server's refusal in red, which stays
  until the next save. A save that went through closes the editor and the page shows the text as the server has it.
  Every editor in place is drawn with `inline_edit.rs`.
- Rules are cards: each `## name` section of a rules file is a white card with its name in the code font, its text and
  its refusals this week, and a pencil next to the name. The pencil grows that card in place, its line turning teal and
  2 px wide, into the editor of the name and the text with Save, Cancel and, at the right, "Delete rule", which asks
  "Delete this rule?" first. Under the last rule an "Add a rule" card on the lane's grey grows into the same editor. The
  server's rules and each repository's have their cards; "Let the Moebius agent write them" is a white button next to
  the heading of the server's rules.
- Budgets and usage are counted in tokens, never in money.
