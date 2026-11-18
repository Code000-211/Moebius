# Design drafts for people, projects and planning

HTML drafts of the screens [the plan for people, projects, the wiki and planning](../plan/issues-people-wiki-planning.md)
adds, to choose between variants. Rounds 1 to 7 chose the patterns (where things are and how they behave) and are drawn
with the first standard's tokens; rounds 8, 9 and 9b chose the look that replaces it
([ADR 0020](../adr/0020-a-quiet-standard-with-depth.md)), so the patterns of rounds 1 to 7 are to be drawn as round 9b
draws them. They are drafts, not
the standard: what is chosen here goes into [`design-web.md`](../design-web.md) with the issue that builds it. Patterns
that trackers share are fair to use; no tracker's look, text or icons are copied.

| # | Pattern | Chosen |
|---|---|---|
| 1 | Sidebar | A project switcher at the top of the list; the chosen project's places under it, the organisation's below ([round 1](round-1.html), 1B) |
| 2 | A card's properties | A list of properties, each edited in place (2A); since round 7 in the right sidebar rather than in the dock |
| 3 | Filters | Filter chips with "+ Filter", a Display menu for grouping, sorting and what is shown, and Save view (3A) |
| 4 | Making a card | Both: a dialog on `C` with properties as chips and "Create more", and "+ Add" at the foot of each column (4A and 4B) |
| 5 | Priority | Signal bars for low, medium and high; only urgent is coloured ([round 2](round-2.html), 5A) |
| 6 | A card on the board | Lean by default; more fields switched on per view in Display (6C) |
| 7 | Changing several cards | Both: checkboxes and shift-click select, a bar floats at the foot, a right click opens the same commands with their keys (7A and 7B) |
| 8 | A cycle's page | Burn-up and figures (done points, cards by agents, tokens per point) above the list (8B) |
| 9 | Inbox | A list with tabs (All, Mentions, Changes, Proposals, Intake); a click opens the card in the dock ([round 3](round-3.html), 9B) |
| 10 | My work | Tabs: Assigned, Created, Following, across projects (10A) |
| 11 | A wiki page | Three columns: the tree, the page with its backlinks under it, and the page's contents at the right (11A) |
| 12 | Command palette | The commands of the card in focus first, with their keys, and recent places below (12B) |
| 13 | A link to a card (`#18`) | A preview card after a short hover or on focus, with project, kind, column, priority, estimate, people and a running run; a click opens the dock ([round 4](round-4.html), 13A) |
| 14 | A card's history | One timeline of comments, runs, moves and field changes, small changes folded together (14A) |
| 15 | Milestones | A list with progress by default, and a timeline as a layout in Display (15A and 15B) |
| 16 | Notifications | Only the inbox: assignments and mentions are its items, its count in the sidebar; no bell (16A) |
| 17 | The whole sidebar | Inbox, My work, Chat and Fleet at the top; the project switcher with the project's Board, Cycles, Wiki, Intake and Files; then Plan (Views, Milestones, Wiki) and Automate (Automations, Workflows, and Improve as one row with Retro, Experiments and Decisions under it); Setup stays behind the gear ([round 5](round-5.html), 17B) |
| 18 | The board | Two lines: the title with the layout switch, Terminal and New issue; under it the filter chips, Display and Save view, which replace the four selects (18A) |
| 19 | The inbox | Quiet rows without buttons, an unread dot, the decision taken in the dock (19B) |
| 20 | Fleet | A compact list, one row per run with its current step, grouped by agent, filter chips, several runs selected and stopped together (20B) |
| 21 | Projects | Tiles for the projects (repositories, members, open cards, private); a project's settings in tabs: General, Repositories, Members, Workflow, Kinds, Plugins ([round 6](round-6.html), 21A) |
| 22 | Pages with a list and a detail (Agents, Skills, MCP servers, Automations, Retro, Experiments) | The list, what is read in the middle, and its properties, edited in place, in the right sidebar (22A, with round 7) |
| 23 | Workflows | A list with trigger, nodes and last run, filter chips and Display (23A) |
| 24 | A run's page | As today, the facts of the run as one line in the head and the conversation at full width; Stop in the head (24B) |
| 25 | Rules | The list of rules, the rule's text in the middle, its scope, checker, refusals and last edit in the right sidebar ([round 7](round-7.html), 25A) |
| 26 | Chat | As today, with Mine and Shared over the list of chats, Share in the head, and the preview card on links such as `#41` (26A) |
| 27 | Usage | As today with filter chips (project, agent, person) over every figure, and sections by project and by person whose rows set the filter (27A and 27B) |
| 28 | The right sidebar | Properties are a sidebar at the right edge of the window, the whole height, foldable, on every page that has them: a card, a run's facts excepted (24B), and every list-and-detail page ([round 7b](round-7b.html), [7c](round-7c.html)) |
| 29 | The Moebius agent | No floating button: "Ask Moebius" is the last row of the right sidebar (`Ctrl J`). Opened, the chat lies over the sidebar at its place and height, wider than it (about 400px) over the page; "Properties" brings the sidebar back. A page without properties has the sidebar folded to a rail with the button. The package sheet opens in the same place ([7c](round-7c.html)) |

The other pages follow these choices without drafts of their own: Models, Plugins and Settings keep their shape and
use the shared filter chips where they list things; Decisions gets the filter chips for project and agent; Get started,
Terminal and Ports stay as they are. Every page's commands are in the command palette (12B).

## The look (rounds 8 and 9)

| # | Lever | Chosen |
|---|---|---|
| 30 | Foundation | Warm greys, 13px body, radii 6 and 8, rows of 28 to 36px, one head bar of 44px ([round 8](round-8.html)) |
| 31 | Colour | Only small status icons (column, urgent, blocked, errors) and one accent for links, the chosen thing, focus and a running run |
| 32 | Card keys | The project's key: `WEB-24`; a forge's issue keeps its number beside it |
| 33 | Depth | The page as a surface on the sidebar, few soft lines, a primary button with slight relief ([round 9](round-9.html)) |
| 34 | Cards | A quiet line and a short soft shadow that grows on hover; no gradient, no light edge ([round 9b](round-9b.html), K3) |
| 35 | Glass | The palette, menus, dialogs and the head bar translucent with a blur |
| 36 | Motion | 120 to 240ms ease-out: a 1px lift on hover, the sliding right sidebar, the palette opening with a small scale, tooltips after a moment |
| 37 | Typography | Inter with optical sizes, tighter tracking in titles, tabular figures |
| 38 | Light and detail | A faint accent wash at the top of the content, a soft glow on the chosen card, a soft focus ring, shortcuts in tooltips, even wrapping, accent selection |
| 39 | The left sidebar's texture | A fine grain and light from above; the organisation and the chosen row as slightly raised surfaces; group names in small capitals ([round 9c](round-9c.html), T4) |
| 40 | The edge between the sidebar and the page | Flush, no line, the page casting a soft shadow onto the sidebar ([round 9d](round-9d.html), E2), with rounded corners so the page does not sit on it square: 6px of air above, below and at the right, all four corners round ([round 9e](round-9e.html), R2) |
| 41 | The right sidebar in the new look | Part of the same surface as the page: its right column, set apart by a soft line, with a head at the height of the page's; folded, and on a page without properties, it stays as a slim column of 44px with the button that unfolds it and the agent's button; "Ask Moebius" is its last row. A second floating surface (B) was tried and looked like two windows ([prototype](prototype.html), A) |

The [prototype](prototype.html) shows every page in the chosen look, with the palette, the agent, menus, previews and the
bar for several cards.
