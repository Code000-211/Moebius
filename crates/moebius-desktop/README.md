# moebius-desktop

The Moebius desktop app, built with [iced](https://iced.rs) to [`docs/design.md`](../../docs/design.md). It talks to a
running server only through the [desktop API](../../docs/reference/desktop-api.md) and does not depend on
`moebius-core`. Its tests start a real server through a dev-dependency.

```sh
cargo run -p moebius-desktop
```

## Layout

| Path | What it holds |
|---|---|
| `src/app/` | The state, every message, how a message changes the state, and how the window is drawn. |
| `src/client/` | The desktop API client: the types, the calls (`client/calls/`), the event stream and the terminal socket. |
| `src/settings.rs` | The saved address and token, the servers connected to before and the SSH preferences. |
| `src/ssh/` | Connecting over SSH: the SSH configuration, the askpass, the setup of the host and the tunnel. |
| `src/theme.rs` | Every colour, size and font. |
| `src/sidebar.rs`, `src/search.rs` | The sidebar on every screen. |
| `src/screens/` | One module or folder per page. |
| `src/widgets/` | The pieces that screens share, and the floating chat. |
| `assets/`, `fonts/` | The icon and the bundled fonts (Manrope and JetBrains Mono). |
| `tests/render/` | Render tests, one module per screen, compared against the pictures in `tests/snapshots/`. Delete a picture to store a new one. |
| `tests/server/` | Tests against a real server, one module per area. |

Every module is described in the [module map](../../docs/architecture.md#modules).
