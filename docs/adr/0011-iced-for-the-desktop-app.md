# The desktop app is built with iced, and its terminal on alacritty_terminal

The desktop app is written with iced, the Rust UI framework that COSMIC is built on, rendered on the GPU through wgpu. Its
terminal (the shell into a run's worktree in the live view) uses `alacritty_terminal` for the emulation, through the
`iced_term` widget, so that Moebius neither writes a terminal emulator of its own nor takes code from COSMIC Terminal.

## Considered Options

- gpui, Zed's framework: very fast on large code and log views, but the terminal widget would have to be written on
  `alacritty_terminal` from scratch.
- Embedding COSMIC Terminal: its code is under GPL-3.0, which Moebius would then have to take on, and it is an application,
  not a widget to put inside another window.
- A terminal emulator of our own: months of work on escape sequences, Unicode and full-screen programs, for nothing the
  existing emulator does not already do.

## Consequences

- The terminal behaves like Alacritty's and looks close to COSMIC Terminal, under licences (Apache-2.0, MIT) that keep
  Moebius free to choose its own.
