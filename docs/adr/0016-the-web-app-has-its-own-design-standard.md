# The web app has its own design standard

Amends the Consequences of [0013](0013-the-web-app-is-the-front-end.md). That record expected the theme's source of
truth to move from `docs/design.md` to `web/src/styles/` once the iced app is removed, and until then `CLAUDE.md` and a
test kept the web's tokens in step with `theme.rs`. This record ends that arrangement now rather than at the removal. A rework of the web app's design (PR 173) wrote a standard of
its own for the web app, and the two looks are no longer the same one.

## Considered Options

- Keep one look and a test that compares `theme.rs` with `theme.css`: the web app would stay tied to a look drawn for a
  native window, and every change to it would need the same change in a crate that is going to be removed.
- Rework the iced app to the new look: the iced app gets nothing new (0013), so the work would go into the part that
  is leaving.
- Let each front end have its own standard, and drop the test that compared them.

## Decision

- The web app is built to [`docs/design-web.md`](../design-web.md): voice, type, colour, components and interaction. Its
  tokens are `web/src/styles/` (`theme.css`, `tailwind.css`), and a test of its own (`styles/tokens.test.ts`) keeps
  them inside that standard.
- The desktop app keeps [`docs/design.md`](../design.md) and `theme.rs`, and its picture tests, as they are until it is
  removed.
- The test that compared `theme.rs` with `theme.css` is dropped. Neither side reads the other.

## Consequences

- A change of colour, size or type in one front end does not touch the other, and nothing fails when they differ.
- When the iced app is removed (0013), `docs/design.md` goes with it and `docs/design-web.md` is the only design
  standard; nothing moves, because the web app's tokens are already where they are to stay.
- The words of the interface may differ between the front ends. The names of the board's columns and tags are the
  server's contract, not a front end's choice: the first column after the Backlog is `Not started` (`Ready` is still
  accepted as its name), and the web app's tag for a card that waits for a person is "Your turn".
