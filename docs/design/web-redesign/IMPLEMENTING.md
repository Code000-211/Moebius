# Implementing an issue of the web redesign

How every issue of the redesign is worked, so each session does it the same way. The standard is
[`docs/design-web.md`](../../design-web.md); what was decided is in [`DECISIONS.md`](DECISIONS.md), with the mockups
beside it. This page goes with the transition note of the standard when the last issue has landed.

## Before you start

Read the issue, `CLAUDE.md`, the standard, `DECISIONS.md` (it lists known mistakes in the mockups) and the mockups the
issue names. The issue's dependencies are merged. Other issues of the redesign run at the same time in other sessions:
stay inside the files and the scope your issue names, and when main moves under you, merge `origin/main` into your
branch (no rebase).

Things that changed under earlier issues and that later ones should know: a page has no sentence under its title any
more, so no test may wait for one; the status icons are five (`StatusIcon`), still, with test ids `status-<kind>`;
icons are imported from `web/src/ui/icons`; errors and confirmations are toasts; the shared primary button is indigo.

## Rules

- Do everything the issue's Acceptance asks, each behaviour with a test, and nothing beyond its scope. Where the issue
  and the standard disagree, the standard wins; where the standard is silent, take the mockup; say in the pull request
  what you decided.
- Any helper or reviewer you start runs with the same settings as you. No model or tool name goes into code, commits,
  the pull request or comments.
- Keep the checks light: the full suites and CI run once, at the end of the whole redesign. Once, before you open the
  pull request, run what covers your change: `cargo fmt`, `cargo clippy` and the tests of the crates you touched if you
  touched Rust; in `web/`: `pnpm check`, the Vitest files of the folders you touched, `pnpm build`, and only the
  end-to-end files that cover the pages you changed (`--trace off`). Wait until they have finished before you report.

## The visual check

The result has to look like the mockups, so every issue is checked by eye.

1. Start the built web app against the fake server the end-to-end tests use and take Playwright screenshots
   (viewport 1280x720, `deviceScaleFactor` 2) of every page or state you change, in the light and the dark theme, and
   of one page you did not mean to change.
2. Look at each screenshot next to the mockups that apply and write down every visible difference: layout, spacing,
   sizes, colours, fonts, radii, icons, what is shown and what is not, anything clipped, overlapping or misaligned,
   anything on screen that helps nobody.
3. Start one fresh helper as an independent reviewer. Give it only the paths of your screenshots, the paths of the
   mockups, the standard and the issue's Acceptance, not your own opinion. It looks at the images and lists every
   difference from the mockups and the standard, the worst first, and names what is outside this issue's scope.
4. Fix every difference that is in scope, take the screenshots again, and repeat from step 2 until the reviewer finds
   none. Differences that belong to a later issue stay, and you list them.
5. Do not commit the screenshots.

## Finishing

- Commit with ordinary subjects and push your branch. Open a pull request against main with the issue's title. Its
  body starts with `Closes #<n>` and has the sections What, Why, Visual check (what you compared with which mockups,
  how many rounds, what the reviewer found, what you fixed, what is left for later issues) and Checks (exactly what you
  ran and what you did not).
- Do not merge and do not wait for CI on the pull request; the session that started you does both.
- When your own checks have finished, send one message to the session that started you: the issue number, the pull
  request number, the full 40-character head commit, one line on the visual check, and anything it must decide. If you
  are blocked, send that instead. Then stop.
