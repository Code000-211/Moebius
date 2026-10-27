# Desktop app design

The look the desktop app is built to, as agreed on the mockup
([Moebius Desktop](https://claude.ai/artifact/4mxF2AbS9tNP39BJyRNPGJ): board, live view, inbox, agents and usage).

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

- A sidebar on every screen, in groups divided by lines: Inbox; Boards; Build (agents, skills, MCP servers, rules);
  Improve (Retro, experiments, decision model, usage); Setup (models, plugins, settings); tokens used today at the foot.
  It collapses to a 68 px rail of icons.
- Every page opens with the same header: an optional breadcrumb, the title, one line of context, and its actions on the
  right.
- Budgets and usage are counted in tokens, never in money.
