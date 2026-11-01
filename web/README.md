# Moebius web app

The web app is the desktop app's counterpart in the browser: React, TypeScript and Tailwind, built into `dist/` and served
by the Moebius server at `/`. It talks to the server only through the [desktop API](../docs/reference/desktop-api.md),
as the iced app does, and signs in with a cookie (`POST /v1/session`). This folder is the scaffold: the shell (sign-in,
sidebar, command bar, connection banner, live data) with the pages of the desktop app as placeholders. The pages come one
by one in later issues.

## Layout

| Path | What it holds |
|---|---|
| `src/main.tsx` | The entry: takes `?login=` out of the address, then starts the router and the query cache. |
| `src/app/` | `App.tsx` (the gate: sign-in, unreachable, shell), `routes.tsx`, `pages.ts` (the sidebar's entries), `SignIn.tsx` and `shell/` (sidebar, command bar, banner, page frame). |
| `src/api/` | The client of the desktop API: `http.ts` (requests and `ApiError`), `calls/` (one module per part of the API, as `crates/moebius-desktop/src/client/calls/`), `types/` (written by hand from the reference), `events.ts` (the WebSocket), `keys.ts` (query keys). |
| `src/live/` | The event stream feeding the query cache (`invalidation.ts`) and the connection state for the pages (`LiveProvider.tsx`). |
| `src/auth/login.ts` | The `?login=<code>` link the server prints. |
| `src/ui/` | Components copied from Vibe Kanban's `@vibe/ui`, each with a header comment saying where from and what changed. |
| `src/styles/` | `theme.css` (Deep Velvet as CSS variables), `tailwind.css` (the Tailwind theme), `fonts.css`. |
| `e2e/` | The Playwright tests and the server they run against. |
| `public/fonts/` | Manrope and JetBrains Mono, the files of the desktop app. |

## Scripts

Install with `pnpm install` (pnpm 10, Node 22). From this folder:

| Script | What it does |
|---|---|
| `pnpm dev` | Vite's dev server with hot reload. It proxies `/v1`, the event WebSocket included, to `MOEBIUS_URL` (default `http://127.0.0.1:7171`). |
| `pnpm build` | Type-checks and builds into `dist/`, which the server embeds. |
| `pnpm check` | `tsc --noEmit` and Biome (lint, format and import order). `pnpm format` fixes what Biome can. |
| `pnpm test` | Vitest: the client's errors, the events-to-cache logic, the `?login` handling, the sign-in page, and a test that `theme.css` matches `theme.rs`. |
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

The tokens are the constants of `crates/moebius-desktop/src/theme.rs`, which stays the source of truth (see
[`docs/design.md`](../docs/design.md)). `src/styles/theme.css` has them as CSS variables under the same names in kebab
case, and `src/styles/tailwind.css` makes them Tailwind colours, radii and spacing. `src/styles/theme.test.ts` reads
`theme.rs` and fails when a colour is missing or differs, or a radius or a step of the spacing scale changes. Change
`theme.rs` first, then `theme.css`. Dark only.

## What changed from Vibe Kanban's components

The components of `src/ui/` were written for React 18 and Tailwind 3. Here: `forwardRef` is gone in `Button` and `Input`
(React 19 passes `ref` as a prop) and `React.ElementRef` is `React.ComponentRef`; Tailwind's `tailwindcss-animate` is
`tw-animate-css`; the colour names the components use (`text-high`, `bg-panel`, `text-brand`, ...) are aliases of the Deep
Velvet colours in `tailwind.css` instead of Vibe Kanban's HSL tokens; `react-i18next`, Tauri helpers and `lucide-react` are
gone (the icons are Phosphor's, the text plain English); `cn` merges Tailwind classes itself. Nothing that reports to
a service (PostHog, Sentry) is copied. Vibe Kanban's `CommandBar` is not copied: it is made of issues, branches and priorities
that Moebius does not have, so `app/shell/CommandBar.tsx` builds the page switcher from its `Command` primitives. Later issues
copy more components (board, chat, terminal) the same way.

## Licenses

The app is Apache-2.0 like the rest of Moebius. `NOTICE` and `LICENSE-APACHE-2.0` are for the Vibe Kanban components. The
fonts are under the SIL Open Font License 1.1 (`public/fonts/*/OFL.txt`).

Direct dependencies, all checked with `pnpm licenses list`; what goes into `dist/` is MIT, except `class-variance-authority`
(Apache-2.0) and `tslib` (0BSD, through Radix):

| Package | License | Used for |
|---|---|---|
| `react`, `react-dom` | MIT | The UI |
| `react-router` | MIT | Routing (lighter than TanStack Router's generated route tree for this many flat routes) |
| `@tanstack/react-query` | MIT | Server data and the cache the events feed |
| `@radix-ui/react-dialog`, `-dropdown-menu`, `-tooltip`, `-slot` | MIT | Behaviour of the copied components |
| `cmdk` | MIT | The command bar |
| `@phosphor-icons/react` | MIT | Icons |
| `class-variance-authority` | Apache-2.0 | Button variants |
| `clsx`, `tailwind-merge`, `tw-animate-css` | MIT | Class names and animation utilities |
| `vite`, `@vitejs/plugin-react` | MIT | Build |
| `tailwindcss`, `@tailwindcss/vite` | MIT | Styles (its CSS engine `lightningcss` is MPL-2.0 and only runs at build time) |
| `typescript` | Apache-2.0 | Types |
| `@biomejs/biome` | MIT OR Apache-2.0 | Lint and format |
| `vitest`, `jsdom`, `@testing-library/*` | MIT | Unit tests |
| `playwright` | Apache-2.0 | End-to-end tests (version pinned to the one preinstalled in the development image) |
| `ws` | MIT | The fake server's WebSocket |

Other transitive licenses in the tree: ISC, BSD-2/3-Clause, MIT-0, BlueOak-1.0.0 and CC0-1.0, all for development tools.

## Later improvements

- The API types in `src/api/types/` are written by hand from `docs/reference/desktop-api.md`. Generating them from the Rust
  types would remove the chance of drift; that is out of scope for the scaffold.
- The sidebar's token meter and the search over cards wait for the Usage page and the board.
