# Navigation by what you do: Settings holds only settings, projects own their things

Supersedes the Shell and Settings sections of [`design/web-redesign/DECISIONS.md`](../design/web-redesign/DECISIONS.md)
and amends [0016](0016-the-web-app-has-its-own-design-standard.md), [0020](0020-a-quiet-standard-with-depth.md) and
[0023](0023-a-quieter-light-first-standard.md) where they list the sidebar. The look of those records stands.

[0023](0023-a-quieter-light-first-standard.md) shrank the sidebar by moving about twenty areas into one Settings page.
That made the sidebar short but Settings a drawer: Agents, Rules, Workflows, Schedules, Analytics, Retro, Lessons and
Export sit next to Profile and Appearance although a person builds with the first and reads the second, and uses them far
more often than a setting. At the same time a project ([0019](0019-a-project-is-a-board-over-any-repositories.md)) owns a
board, repositories, columns and members, yet its Cycles, Milestones and intake were entries of the Workspace and of
Settings, and its repositories were one list for the whole server. A person had to know which part of the product
a thing belonged to by its implementation, not by what they were doing.

The rule that replaces it: **place a thing where you do what you do with it.** Settings holds what one sets and then
leaves alone. What one builds, reads or works in has its own place, and everything a project owns is in the project.

The approved mockups are in [`design/navigation/`](../design/navigation/README.md): `ia1.png` (a project with its tabs
and the gear), `ia2.png` (the project's Workflows tab), `ia3.png` (project settings) and `ia4.png` (Settings). They are
the reference for layout; where a mockup and this record differ, this record wins.

## Considered Options

- **Keep one Settings page for everything rare** (0023). Rejected: it mixes settings with tools and reports, and it hides
  what a project owns.
- **Build and Insight as one entry with tabs.** Rejected: building (Agents, Rules) and reading (Insights, Improve) are
  different activities and two short groups read faster than one entry with tabs behind it.
- **Keep workflows server-wide and show a filtered run view in each project.** Rejected: a project would show things it
  does not own and the filter would be the only link. Workflows belong to a project (ADR 0025, written with the change that
  implements it).
- **A cross-project timeline of Cycles and Milestones.** Rejected: a cycle belongs to one project's board; a second place
  for the same thing, and Fleet already answers what runs now.
- **Sidebar entries for each project's parts.** Rejected: it multiplies entries by the number of projects. A project has
  tabs.

## Decision

- **A project owns its things, behind tabs and a gear.** The project page has the tabs **Board | Workflows | Cycles |
  Milestones | Intake**; Intake shows only when the project has intake on. A gear in the header opens the project's
  settings: **General | Members | Repositories | Columns**, and the switch for intake. A repository is a page under its
  project, not a list of the server.
- **The sidebar** is, from the top: the workspace switch with search; **Inbox** (count); **My issues**; **Fleet** (tabs
  Runs | Runners | Ports); **Chat**; **Projects**; **Workspace** (Wiki, Views, with *All issues* as a view); **Build**
  (Agents, with tabs Skills and MCP servers; Rules); **Insight** (Insights, Improve); **Settings** at the bottom. Workflows
  are not under Build, because a workflow belongs to a project.
- **Settings holds only settings.** Two groups: **You** (Profile, Appearance, Keyboard shortcuts, My tokens) and
  **Server** (General, Models, Plugins, Kinds of issues, Members). Nothing in it is a tool or a report.
- **Cycles, Milestones and Intake exist only in the project.** No sidebar or Settings entry reaches them across projects.
- **Removed as places of their own:** More, Notifications (the Inbox is the place), Schedules (a schedule is a trigger of a
  workflow, in its project), Settings > Repositories (now the project's), Settings > Runners and Ports (now Fleet tabs),
  Build > Workflows, and the cross-project Cycles, Milestones and Intake entries. Column scripts live with the project's
  Columns.
- **Terminal and the quick window** are not sidebar entries. They are in the command bar and in the project's `...` menu.
- **Legacy paths redirect.** Every removed path keeps working: it redirects to the page that now holds the thing, and a
  path whose owner cannot be known (a workflow or cycle without a project in the link) redirects to the project list. The
  mapping is one table in the web app, with a test that every removed path is in it, so a bookmark, a link in an issue
  and a notification written by an older version still arrive.
- **One navigation table.** The sidebar, the command bar and the redirects read the same table; no page lists itself in
  two places.

## Consequences

- `design/web-redesign/DECISIONS.md` (Shell, Settings) and `design-web.md` are edited by the issues that change the matching
  screen; those files say where they changed and point here. Until then the code and those files still show 0023's sidebar.
- The web app's routes split into the app's own and the project's; the board is addressed by project.
- Workflows moving into projects, with the migration of existing ones, is a separate record (ADR 0025).
- The screenshots of the guide are taken again when the pages have moved.
